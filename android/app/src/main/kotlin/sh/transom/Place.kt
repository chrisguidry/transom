package sh.transom

import java.io.Serializable

/**
 * A screen of the signed-in app, other than the inbox, which is home. The app's places are a list,
 * the last on screen and the system's back gesture going to the one before, with the inbox under
 * them all.
 *
 * Each is [Serializable], so the list is saved with the activity, as when the phone turns, with no
 * routes to write and read back.
 */
sealed interface Place : Serializable {
    /** The exchange [id]. */
    data class Exchange(val id: String) : Place

    /** The members of [group], a name without its `@`. */
    data class Members(val group: String) : Place

    data object Settings : Place

    /** This identity's devices, from Settings. */
    data object Devices : Place

    /** The licenses of the libraries the app is built with, from Settings. */
    data object Licenses : Place
}
