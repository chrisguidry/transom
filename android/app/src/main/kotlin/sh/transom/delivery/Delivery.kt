package sh.transom.delivery

import androidx.annotation.StringRes
import sh.transom.R

/** How the person chose, in Settings, that notifications reach this phone. */
enum class Delivery {
    /**
     * The server wakes the phone through FCM, as an `{"fcm": token}` device, and the phone holds no
     * connection to the server.
     */
    GooglePlay,

    /**
     * The phone holds a connection to the server, as a `{"connection": true}` device, and Google is
     * not involved.
     */
    Connection,
}

/** Where delivery to this phone stands, as the phone's registration as a device settled it. */
sealed interface DeliveryState {
    /** The phone holds its connection to the server. */
    data object Connection : DeliveryState

    /** Google Play services is chosen, and the registration has not settled yet. */
    data object Registering : DeliveryState

    /** The server took FCM's [token], and wakes the phone through it. */
    data class Delivering(val token: String) : DeliveryState

    /**
     * Google Play services is chosen and does not work, for the [reason] Settings and the
     * notification show. Nothing reaches the phone while the app is closed.
     */
    enum class Failing(@param:StringRes val reason: Int) : DeliveryState {
        /** FCM gave no token. */
        NoToken(R.string.delivery_no_token),

        /** The server refused the token, with a refusal that would come again, such as 422. */
        Refused(R.string.delivery_refused),
    }
}
