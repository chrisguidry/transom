package sh.transom.delivery

import android.content.Context
import androidx.core.app.NotificationCompat
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.ForegroundInfo
import androidx.work.ListenableWorker
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.WorkManager
import androidx.work.WorkerFactory
import androidx.work.WorkerParameters
import sh.transom.R
import sh.transom.notifications.Channels

/**
 * A wake-up's work: [catchUp], usually [sh.transom.Account.catchUp], which refreshes the copy from
 * the server and posts what arrived, and asks to run again later when it returns `false`.
 */
class CatchUpWorker(
    context: Context,
    parameters: WorkerParameters,
    private val catchUp: suspend () -> Boolean,
) : CoroutineWorker(context, parameters) {
    override suspend fun doWork(): Result = if (catchUp()) Result.success() else Result.retry()

    /**
     * The notification Android 11 and earlier show while the work runs, since WorkManager runs
     * expedited work there as a foreground service. It goes on the background connection's quiet
     * channel.
     */
    override suspend fun getForegroundInfo(): ForegroundInfo =
        ForegroundInfo(
            NOTIFICATION,
            NotificationCompat.Builder(applicationContext, Channels.CONNECTION)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentTitle(applicationContext.getString(R.string.catch_up_title))
                .setSilent(true)
                .setShowWhen(false)
                .build(),
        )

    companion object {
        /** The name of the work that catches up, of which WorkManager keeps one. */
        const val NAME = "catch-up"

        /** The id of the work's notification, which no other notification has. */
        const val NOTIFICATION = 3

        /**
         * Asks [workManager] to catch up as soon as the phone is online, as expedited work, which
         * Android runs at once while the high-priority FCM message that asked for it lets the app
         * run. A wake-up that arrives while one runs replaces it, since each refresh reads
         * everything the server lists at the time.
         */
        fun schedule(workManager: WorkManager) {
            val request =
                OneTimeWorkRequestBuilder<CatchUpWorker>()
                    .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
                    .setConstraints(Constraints(requiredNetworkType = NetworkType.CONNECTED))
                    .build()
            workManager.enqueueUniqueWork(NAME, ExistingWorkPolicy.REPLACE, request)
        }
    }
}

/**
 * Makes the [CatchUpWorker] with what it catches up with, which WorkManager's own factory cannot
 * give it. The app's WorkManager configuration names this factory.
 */
class CatchUpWorkerFactory(private val catchUp: suspend () -> Boolean) : WorkerFactory() {
    override fun createWorker(
        appContext: Context,
        workerClassName: String,
        workerParameters: WorkerParameters,
    ): ListenableWorker? =
        if (workerClassName == CatchUpWorker::class.java.name) {
            CatchUpWorker(appContext, workerParameters, catchUp)
        } else {
            null
        }
}
