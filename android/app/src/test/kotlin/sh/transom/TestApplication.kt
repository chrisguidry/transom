package sh.transom

import androidx.work.Configuration
import androidx.work.testing.SynchronousExecutor
import androidx.work.testing.WorkManagerTestInitHelper
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey

/**
 * The app as the tests run it, sealing the session with an AES-256 key from the JVM.
 *
 * WorkManager keeps one instance for the whole process, which outlives each test's application, so
 * each test gets a test WorkManager of its own, with the app's configuration. Its work waits for
 * the phone to be online until a test says it is, with `online()`.
 */
class TestApplication : TransomApplication() {
    override fun sessionKey(): SecretKey = key

    private val key = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()

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
