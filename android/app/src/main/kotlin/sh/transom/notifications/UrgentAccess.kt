package sh.transom.notifications

import android.annotation.SuppressLint
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.provider.Settings
import androidx.core.net.toUri

/**
 * Whether an urgent notification may show full screen over the lock screen. Android 14 and later
 * grant that only to calling and alarm apps unless the person allows it in the app's settings;
 * earlier releases grant it with the app's install.
 */
internal fun Context.mayShowFullScreen(): Boolean =
    Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE ||
        getSystemService(NotificationManager::class.java).canUseFullScreenIntent()

/**
 * Whether the person gave the app access to Do Not Disturb, which an urgent channel needs to
 * override it.
 */
internal fun Context.mayOverrideDoNotDisturb(): Boolean =
    getSystemService(NotificationManager::class.java).isNotificationPolicyAccessGranted

/** Android's list of the apps that may change Do Not Disturb, where the person allows transom. */
internal fun doNotDisturbAccess(): Intent =
    Intent(Settings.ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS)

/**
 * Android's page for this app's full-screen notifications. It exists from Android 14, the first
 * release that asks, and the app only offers it where [mayShowFullScreen] is false.
 */
@SuppressLint("InlinedApi")
internal fun Context.fullScreenAccess(): Intent =
    Intent(Settings.ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT, "package:$packageName".toUri())
