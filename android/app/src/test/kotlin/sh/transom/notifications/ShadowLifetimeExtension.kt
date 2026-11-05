package sh.transom.notifications

import android.app.Notification
import android.app.NotificationManager
import java.util.concurrent.ConcurrentHashMap
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.annotation.Resetter
import org.robolectric.shadows.ShadowNotificationManager

/**
 * Android's lifetime extension for direct replies, which Robolectric leaves out. Android flags the
 * notification the person answered from its reply box, and drops the app's cancel of it while the
 * notification it shows carries the flag. An update from the app without the flag clears it once
 * Android posts the update, about 200 ms after the app asks, so a cancel sent sooner is dropped
 * too.
 */
@Implements(NotificationManager::class)
class ShadowLifetimeExtension : ShadowNotificationManager() {
    @Implementation
    override fun notify(tag: String?, id: Int, notification: Notification) {
        super.notify(tag, id, notification)
        val key = tag ?: return
        if (notification.flags and LIFETIME_EXTENDED != 0) {
            extendedUntil[key] = Long.MAX_VALUE
        } else {
            extendedUntil.computeIfPresent(key) { _, until ->
                minOf(until, System.nanoTime() + LAG)
            }
        }
    }

    @Implementation
    override fun cancel(tag: String?, id: Int) {
        val until = tag?.let { extendedUntil[it] }
        if (until != null && System.nanoTime() < until) return
        tag?.let { extendedUntil.remove(it) }
        super.cancel(tag, id)
    }

    companion object {
        // Notification's FLAG_LIFETIME_EXTENDED_BY_DIRECT_REPLY, which the SDK hides.
        private const val LIFETIME_EXTENDED = 0x00010000
        private const val LAG = 200_000_000L
        private val extendedUntil = ConcurrentHashMap<String, Long>()

        /** Flags exchange [id]'s notification as Android does once the person answers from it. */
        fun extend(id: String) {
            val shown = checkNotNull(posted(id))
            shown.flags = shown.flags or LIFETIME_EXTENDED
            extendedUntil[id] = Long.MAX_VALUE
        }

        // Named apart from ShadowNotificationManager's own reset, which this one replaces.
        @Resetter
        @JvmStatic
        fun unextend() {
            extendedUntil.clear()
            ShadowNotificationManager.reset()
        }
    }
}
