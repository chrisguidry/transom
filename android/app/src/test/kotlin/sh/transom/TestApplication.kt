package sh.transom

import androidx.work.Configuration
import androidx.work.testing.SynchronousExecutor
import androidx.work.testing.WorkManagerTestInitHelper
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import sh.transom.delivery.GooglePlay
import sh.transom.delivery.Push
import sh.transom.signin.app

/**
 * The app as the tests run it, sealing the session with an AES-256 key from the JVM.
 *
 * WorkManager keeps one instance for the whole process, which outlives each test's application, so
 * each test gets a test WorkManager of its own, with the app's configuration. Its work waits for
 * the phone to be online until a test says it is, with `online()`.
 *
 * Google Play services is unavailable to it, as to a build without the Firebase project's values,
 * until a test gives it [playServices].
 */
class TestApplication : TransomApplication() {
    override fun sessionKey(): SecretKey = key

    private val key = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()

    /** Google Play services as this build on this phone would have it. */
    var playServices: GooglePlay = GooglePlay.Unavailable.NotInBuild

    /** Gives the app Google Play services, with FCM as [push]. */
    fun fcm(push: Push) {
        playServices = GooglePlay.Available { push }
    }

    override fun googlePlay(): GooglePlay = playServices

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
