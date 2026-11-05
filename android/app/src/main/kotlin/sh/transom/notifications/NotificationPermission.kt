package sh.transom.notifications

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.core.content.edit

/**
 * Whether this device's notifications are [on], which the bell in the top bar shows, and asking for
 * them. Android 13 and later post nothing until the person allows it, and ask at most twice: after
 * the second refusal, only the app's notification settings can turn them on.
 *
 * Make it while the [activity] is being made, since it registers for the permission's answer.
 */
class NotificationPermission(private val activity: ComponentActivity) {
    var on: Boolean by mutableStateOf(false)
        private set

    private val prefs by lazy { activity.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE) }

    // Android's question is an activity of its own, so the app resumes once it closes and reads
    // the answer then, with refresh.
    private val request =
        activity.registerForActivityResult(ActivityResultContracts.RequestPermission()) {}

    /** Reads again whether notifications are on, as when the app comes back from the settings. */
    fun refresh() {
        on = enabled()
    }

    /** Asks for notifications once, after the person signs in, unless they are on already. */
    fun askOnce() {
        if (!on && !asked && Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) ask()
    }

    /**
     * The bell was pressed: asks for notifications while Android still shows its question, and
     * otherwise opens the app's notification settings, where they are turned on or off.
     */
    fun pressed() {
        val askable = !asked || activity.shouldShowRequestPermissionRationale(PERMISSION)
        if (!on && askable && Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) ask()
        else openSettings()
    }

    private val asked: Boolean
        get() = prefs.getBoolean(ASKED, false)

    private fun ask() {
        prefs.edit { putBoolean(ASKED, true) }
        request.launch(PERMISSION)
    }

    /** Opens the app's notification settings, where each urgency's channel is. */
    fun openSettings() {
        activity.startActivity(
            Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                .putExtra(Settings.EXTRA_APP_PACKAGE, activity.packageName)
        )
    }

    private fun enabled(): Boolean {
        val granted =
            Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
                ContextCompat.checkSelfPermission(activity, PERMISSION) ==
                    PackageManager.PERMISSION_GRANTED
        return granted && NotificationManagerCompat.from(activity).areNotificationsEnabled()
    }

    private companion object {
        // Only Android 13 and later have the permission. On earlier releases the app never asks
        // for it, and asking whether to explain it answers false.
        @SuppressLint("InlinedApi") const val PERMISSION = Manifest.permission.POST_NOTIFICATIONS
        const val PREFERENCES = "notifications"
        const val ASKED = "asked"
    }
}
