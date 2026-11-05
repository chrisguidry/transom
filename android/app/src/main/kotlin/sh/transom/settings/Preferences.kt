package sh.transom.settings

import android.content.Context
import androidx.core.content.edit
import sh.transom.delivery.Delivery

/**
 * The choices a person makes in Settings, kept on this phone. They belong to the phone rather than
 * the session, so signing in again keeps them.
 */
class Preferences(context: Context) {
    private val prefs = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

    /**
     * How the person chose that notifications reach this phone: through Google Play services unless
     * they choose the direct connection. A phone without Google Play services uses the direct
     * connection whatever this says.
     */
    var delivery: Delivery
        get() = prefs.getString(DELIVERY, null)?.let(Delivery::valueOf) ?: Delivery.GooglePlay
        set(chosen) = prefs.edit { putString(DELIVERY, chosen.name) }

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
        const val DELIVERY = "delivery"
        const val PLAIN_ACTIONS_WHILE_LOCKED = "plain-actions-while-locked"
    }
}
