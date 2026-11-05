package sh.transom

import android.content.Intent
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.work.WorkInfo
import androidx.work.WorkManager
import androidx.work.testing.WorkManagerTestInitHelper
import kotlinx.coroutines.runBlocking
import org.junit.rules.ExternalResource
import org.robolectric.android.controller.ActivityController
import sh.transom.signin.Session
import sh.transom.signin.app
import sh.transom.signin.awaitMain
import sh.transom.sync.OutboxWorker
import sh.transom.testserver.TestServer

/**
 * The app's activity for a test. After the test, it forgets the app's account, which stops the live
 * feed the inbox follows, so no test's feed keeps reconnecting to a server that has stopped while
 * the next test runs.
 */
class LaunchedApp : ExternalResource() {
    /** Launches the app signed in as [name] on [server], with the issuer's token for [name]. */
    fun signedIn(server: TestServer, name: String): ActivityController<MainActivity> =
        signedIn(Session(server.url, server.tokenFor(name), name))

    /** Launches the app signed in with [session]. */
    fun signedIn(session: Session): ActivityController<MainActivity> {
        app.sessions.keep(session)
        return launch()
    }

    /** Launches the app for [intent], as `sh.transom.signin.launch` does. */
    fun launch(intent: Intent = Intent(app, MainActivity::class.java)) =
        sh.transom.signin.launch(intent)

    override fun after() {
        runBlocking { app.signedOut() }
    }
}

/**
 * Tells WorkManager the phone is online, so the outbox's work that is waiting for it runs. Work
 * that is queued behind other work waits until that work has run, so a test that queues two calls
 * this until both have.
 */
fun online() {
    val driver = checkNotNull(WorkManagerTestInitHelper.getTestDriver(app))
    WorkManager.getInstance(app)
        .getWorkInfosForUniqueWork(OutboxWorker.NAME)
        .get()
        .filter { it.state == WorkInfo.State.ENQUEUED }
        .forEach { driver.setAllConstraintsMet(it.id) }
}

/** Waits for [condition] with the phone online, as the outbox's deliveries settle. */
fun ComposeTestRule.awaitOnline(condition: () -> Boolean) = awaitMain {
    online()
    condition()
}
