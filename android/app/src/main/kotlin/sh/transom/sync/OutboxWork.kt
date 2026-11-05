package sh.transom.sync

import android.content.Context
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.ListenableWorker
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerFactory
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit

/**
 * Runs [deliver], usually [Outbox.deliver] with the signed-in session, when WorkManager says the
 * phone is online, and asks to run again later when it returns `false`.
 */
class OutboxWorker(
    context: Context,
    parameters: WorkerParameters,
    private val deliver: suspend () -> Boolean,
) : CoroutineWorker(context, parameters) {
    override suspend fun doWork(): Result = if (deliver()) Result.success() else Result.retry()

    companion object {
        /** The name of the outbox's work, of which WorkManager keeps one chain. */
        const val NAME = "outbox"

        /**
         * Asks [workManager] to deliver the outbox once the phone is online, after any delivery
         * already asked for. Each run sends everything queued when it starts, so appending one
         * means a reply queued while another run is sending is sent too.
         */
        fun schedule(workManager: WorkManager) {
            val request =
                OneTimeWorkRequestBuilder<OutboxWorker>()
                    .setConstraints(Constraints(requiredNetworkType = NetworkType.CONNECTED))
                    .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS)
                    .build()
            workManager.enqueueUniqueWork(NAME, ExistingWorkPolicy.APPEND_OR_REPLACE, request)
        }
    }
}

/**
 * Makes the [OutboxWorker] with what it delivers, which WorkManager's own factory cannot give it.
 * The app's WorkManager configuration names this factory.
 */
class OutboxWorkerFactory(private val deliver: suspend () -> Boolean) : WorkerFactory() {
    override fun createWorker(
        appContext: Context,
        workerClassName: String,
        workerParameters: WorkerParameters,
    ): ListenableWorker? =
        if (workerClassName == OutboxWorker::class.java.name) {
            OutboxWorker(appContext, workerParameters, deliver)
        } else {
            null
        }
}
