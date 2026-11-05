package sh.transom.settings

import android.content.Context
import androidx.core.content.edit

/**
 * The choices a person makes in Settings, kept on this phone. They belong to the phone rather than
 * the session, so signing in again keeps them.
 */
class Preferences(context: Context) {
    private val prefs = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

    /**
     * Whether the background connection runs while the app is signed in, so questions arrive with
     * the app closed. It is on unless the person turns it off.
     */
    var backgroundConnection: Boolean
        get() = prefs.getBoolean(BACKGROUND_CONNECTION, true)
        set(on) = prefs.edit { putBoolean(BACKGROUND_CONNECTION, on) }

    /**
     * Whether a notification's plain actions, such as Open, work from the lock screen without
     * unlocking the phone. They need it unless the person lifts it. Android always requires
     * unlocking for a chip or typed text, whatever this says.
     */
    var plainActionsWhileLocked: Boolean
        get() = prefs.getBoolean(PLAIN_ACTIONS_WHILE_LOCKED, false)
        set(on) = prefs.edit { putBoolean(PLAIN_ACTIONS_WHILE_LOCKED, on) }

    private companion object {
        const val PREFERENCES = "settings"
        const val BACKGROUND_CONNECTION = "background-connection"
        const val PLAIN_ACTIONS_WHILE_LOCKED = "plain-actions-while-locked"
    }
}
