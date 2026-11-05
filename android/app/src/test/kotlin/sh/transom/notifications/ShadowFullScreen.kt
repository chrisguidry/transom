package sh.transom.notifications

import android.app.NotificationManager
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.annotation.Resetter
import org.robolectric.shadows.ShadowNotificationManager

/**
 * Android 14's permission for full-screen notifications, which Robolectric leaves out and always
 * refuses. A test that runs with this shadow has it granted unless it says otherwise.
 */
@Implements(NotificationManager::class)
class ShadowFullScreen : ShadowNotificationManager() {
    @Implementation fun canUseFullScreenIntent(): Boolean = granted

    companion object {
        /** Whether the person allowed full-screen notifications. */
        @Volatile var granted = true

        // Grants it again for the next test. Named apart from ShadowNotificationManager's own
        // reset, which this one replaces.
        @Resetter
        @JvmStatic
        fun restore() {
            granted = true
            ShadowNotificationManager.reset()
        }
    }
}
