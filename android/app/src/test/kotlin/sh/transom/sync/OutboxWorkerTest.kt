package sh.transom.sync

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.work.Configuration
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkInfo
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import androidx.work.testing.SynchronousExecutor
import androidx.work.testing.WorkManagerTestInitHelper
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** WorkManager runs the outbox once the phone is online, and again later when it asks to. */
@RunWith(RobolectricTestRunner::class)
class OutboxWorkerTest {
    private val context: Context = ApplicationProvider.getApplicationContext()

    /** WorkManager for the test, running [deliver] as the outbox's delivery. */
    private fun workManager(deliver: suspend () -> Boolean): WorkManager {
        val configuration =
            Configuration.Builder()
                .setWorkerFactory(OutboxWorkerFactory(deliver))
                .setExecutor(SynchronousExecutor())
                .build()
        WorkManagerTestInitHelper.initializeTestWorkManager(context, configuration)
        return WorkManager.getInstance(context)
    }

    @After
    fun closeWorkManager() {
        WorkManagerTestInitHelper.closeWorkDatabase()
    }

    /** The outbox's work once it reaches a state that passes [condition]. */
    private suspend fun WorkManager.outbox(condition: (WorkInfo) -> Boolean): WorkInfo =
        getWorkInfosForUniqueWorkFlow(OutboxWorker.NAME)
            .awaiting { infos -> infos.any(condition) }
            .first(condition)

    /** Schedules the outbox, and tells WorkManager the phone is online. */
    private fun WorkManager.online() {
        OutboxWorker.schedule(this)
        val work = getWorkInfosForUniqueWork(OutboxWorker.NAME).get().single()
        WorkManagerTestInitHelper.getTestDriver(context)!!.setAllConstraintsMet(work.id)
    }

    @Test
    fun waitsUntilThePhoneIsOnline() = runBlocking {
        var deliveries = 0
        val workManager = workManager {
            deliveries++
            true
        }
        OutboxWorker.schedule(workManager)
        workManager.outbox { it.state == WorkInfo.State.ENQUEUED }
        assertEquals(0, deliveries)
    }

    @Test
    fun deliversOnceThePhoneIsOnline() = runBlocking {
        val workManager = workManager { true }
        workManager.online()
        workManager.outbox { it.state == WorkInfo.State.SUCCEEDED }
        Unit
    }

    @Test
    fun triesAgainWhenDeliveryAsksTo() = runBlocking {
        val workManager = workManager { false }
        workManager.online()
        val retried = workManager.outbox { it.runAttemptCount == 1 }
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
}

/** Work of some other part of the app, which WorkManager makes itself. */
class Elsewhere(context: Context, parameters: WorkerParameters) : Worker(context, parameters) {
    override fun doWork(): Result = Result.success()
}
