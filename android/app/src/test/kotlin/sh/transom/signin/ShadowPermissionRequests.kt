package sh.transom.signin

import android.content.pm.PackageManager
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements

/**
 * Whether the system's dialog would ask for a permission, as Android's permission service answers.
 * Robolectric has no permission service, so without this shadow Android 17's `requestPermissions`
 * takes every permission as granted and answers at once, without asking. With it, a permission the
 * test has not granted goes to the dialog, which the test answers.
 */
// PermissionManager is hidden from the SDK, so the shadow names it.
@Implements(className = "android.permission.PermissionManager")
class ShadowPermissionRequests {
    @Implementation
    @Suppress("UNUSED_PARAMETER")
    fun getPermissionRequestState(packageName: String, permission: String, deviceId: Int): Int =
        if (
            RuntimeEnvironment.getApplication().checkSelfPermission(permission) ==
                PackageManager.PERMISSION_GRANTED
        ) {
            GRANTED
        } else {
            REQUESTABLE
        }

    private companion object {
        // Context's PERMISSION_REQUEST_STATE_GRANTED and _REQUESTABLE, which the SDK hides.
        const val GRANTED = 0
        const val REQUESTABLE = 1
    }
}
