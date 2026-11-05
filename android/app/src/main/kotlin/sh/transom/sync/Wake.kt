package sh.transom.sync

/** How the server wakes this phone, as the `wake` of the device it registers says. */
sealed interface Wake {
    /**
     * `{"connection": true}`: the phone holds the live feed open in the background, so the server
     * pushes nothing to it.
     */
    data object Connection : Wake

    /**
     * `{"fcm": token}`: the server asks the wake-up gateway to send FCM's registration [token] a
     * message with nothing in it, and the phone then syncs from its server.
     */
    data class Fcm(val token: String) : Wake
}
