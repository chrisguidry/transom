package sh.transom.wake

/**
 * FCM as this phone has it: the token that the server's wake-ups are sent to, which is the Firebase
 * installation ID that FCM registered.
 */
fun interface Push {
    /** The token for this app on this phone, or `null` when FCM could not register it. */
    suspend fun token(): String?
}

/**
 * The Firebase project's values that the app is built with, the outputs of `gateway/firebase.tf`.
 * None of them is secret: Firebase puts them in every app.
 */
data class FirebaseValues(
    val projectId: String,
    val applicationId: String,
    val apiKey: String,
    val senderId: String,
) {
    companion object {
        /**
         * The values, or `null` unless the build has all four. A build without a value has it as an
         * empty string.
         */
        fun of(projectId: String, applicationId: String, apiKey: String, senderId: String) =
            FirebaseValues(projectId, applicationId, apiKey, senderId).takeIf {
                listOf(projectId, applicationId, apiKey, senderId).none(String::isBlank)
            }
    }
}

/**
 * How FCM reaches this phone, or `null` when it cannot and the phone holds its connection to the
 * server instead: FCM needs the Firebase project's [values] in the build and Google Play services
 * on the phone, which [playServices] says. [start] starts Firebase with the values, and is called
 * only when both are there, so a build or a phone without them never starts it.
 */
fun pushFor(
    values: FirebaseValues?,
    playServices: () -> Boolean,
    start: (FirebaseValues) -> Push,
): Push? = values?.takeIf { playServices() }?.let(start)
