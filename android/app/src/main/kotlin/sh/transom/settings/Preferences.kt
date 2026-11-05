package sh.transom.settings

import android.content.Context
import androidx.core.content.edit
import java.time.Instant
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
     * When this phone last heard from its server: an event on the live feed, or a catch-up after a
     * wake, which Settings shows so the person can trust that messages are arriving. The server
     * sends nothing to the phone for this alone.
     */
    var heard: Instant?
        get() = prefs.getLong(HEARD, 0).takeIf { it > 0 }?.let(Instant::ofEpochMilli)
        set(at) = prefs.edit { putLong(HEARD, at?.toEpochMilli() ?: 0) }

    private companion object {
        const val PREFERENCES = "settings"
        const val DELIVERY = "delivery"
        const val HEARD = "heard"
    }
}
