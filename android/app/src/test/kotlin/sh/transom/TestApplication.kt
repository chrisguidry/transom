package sh.transom

import androidx.work.Configuration
import androidx.work.testing.SynchronousExecutor
import androidx.work.testing.WorkManagerTestInitHelper
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import sh.transom.signin.app
import sh.transom.wake.Push

/**
 * The app as the tests run it, sealing the session with an AES-256 key from the JVM.
 *
 * WorkManager keeps one instance for the whole process, which outlives each test's application, so
 * each test gets a test WorkManager of its own, with the app's configuration. Its work waits for
 * the phone to be online until a test says it is, with `online()`.
 *
 * It has no FCM, as a build without the Firebase project's values has none, until a test gives it
 * [fcm].
 */
class TestApplication : TransomApplication() {
    override fun sessionKey(): SecretKey = key

    private val key = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()

    /** FCM as a phone with Google Play services and a build with Firebase would have it. */
    var fcm: Push? = null

    override fun push(): Push? = fcm

    override fun onCreate() {
        super.onCreate()
        WorkManagerTestInitHelper.initializeTestWorkManager(
            this,
            Configuration.Builder(workManagerConfiguration)
                .setExecutor(SynchronousExecutor())
                .build(),
        )
    }
}

/** The app the test runs in, as the tests build it. */
val testApp: TestApplication
    get() = app as TestApplication
