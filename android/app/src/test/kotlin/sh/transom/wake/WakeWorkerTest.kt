package sh.transom.wake

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.work.Configuration
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkInfo
import androidx.work.WorkManager
import androidx.work.testing.SynchronousExecutor
import androidx.work.testing.TestListenableWorkerBuilder
import androidx.work.testing.WorkManagerTestInitHelper
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.Account
import sh.transom.notifications.Channels
import sh.transom.signin.Session
import sh.transom.signin.app
import sh.transom.sync.Elsewhere
import sh.transom.sync.awaiting
import sh.transom.testserver.NOWHERE

/** WorkManager catches up after a wake-up once the phone is online, and again when it must. */
@RunWith(RobolectricTestRunner::class)
class WakeWorkerTest {
    private val context: Context = ApplicationProvider.getApplicationContext()

    /** WorkManager for the test, running [catchUp] as the wake-up's work. */
    private fun workManager(catchUp: suspend () -> Boolean): WorkManager {
        val configuration =
            Configuration.Builder()
                .setWorkerFactory(WakeWorkerFactory(catchUp))
                .setExecutor(SynchronousExecutor())
                .build()
        WorkManagerTestInitHelper.initializeTestWorkManager(context, configuration)
        return WorkManager.getInstance(context)
    }

    @After
    fun closeWorkManager() {
        WorkManagerTestInitHelper.closeWorkDatabase()
    }

    /** The wake-up's work once it reaches a state that passes [condition]. */
    private suspend fun WorkManager.wakeUp(condition: (WorkInfo) -> Boolean): WorkInfo =
        getWorkInfosForUniqueWorkFlow(WakeWorker.NAME)
            .awaiting { infos -> infos.any(condition) }
            .first(condition)

    /** Schedules a wake-up's work, and tells WorkManager the phone is online. */
    private fun WorkManager.woken() {
        WakeWorker.schedule(this)
        val work = getWorkInfosForUniqueWork(WakeWorker.NAME).get().single()
        WorkManagerTestInitHelper.getTestDriver(context)!!.setAllConstraintsMet(work.id)
    }

    @Test
    fun waitsUntilThePhoneIsOnline() = runBlocking {
        var catchUps = 0
        val workManager = workManager {
            catchUps++
            true
        }
        WakeWorker.schedule(workManager)
        workManager.wakeUp { it.state == WorkInfo.State.ENQUEUED }
        assertEquals(0, catchUps)
    }

    @Test
    fun catchesUpOnceThePhoneIsOnline() = runBlocking {
        val workManager = workManager { true }
        workManager.woken()
        workManager.wakeUp { it.state == WorkInfo.State.SUCCEEDED }
        Unit
    }

    @Test
    fun triesAgainWhenTheServerCouldNotBeReached() = runBlocking {
        val workManager = workManager { false }
        workManager.woken()
        val retried = workManager.wakeUp { it.runAttemptCount == 1 }
        assertEquals(WorkInfo.State.ENQUEUED, retried.state)
    }

    @Test
    fun leavesOtherWorkToWorkManager() = runBlocking {
        val workManager = workManager { true }
        val request = OneTimeWorkRequestBuilder<Elsewhere>().build()
        workManager.enqueue(request)
        workManager.getWorkInfoByIdFlow(request.id).awaiting {
            it?.state == WorkInfo.State.SUCCEEDED
        }
        Unit
    }

    @Test
    fun anOlderAndroidShowsAQuietNotificationWhileItRuns() = runBlocking {
        val worker =
            TestListenableWorkerBuilder<WakeWorker>(context)
                .setWorkerFactory(WakeWorkerFactory { true })
                .build()
        val shown = worker.getForegroundInfo()
        assertEquals(WakeWorker.NOTIFICATION, shown.notificationId)
        assertEquals(Channels.CONNECTION, shown.notification.channelId)
    }

    @Test
    fun anAccountWhoseServerCannotBeReachedCatchesUpLater() {
        val account =
            Account(
                Session(NOWHERE, "transom_token", "bob"),
                app,
                WorkManager.getInstance(app),
                OkHttpClient(),
                app.database,
            )
        assertFalse(runBlocking { account.catchUp() })
        runBlocking { account.forget() }
    }
}
