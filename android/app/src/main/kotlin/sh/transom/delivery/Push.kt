package sh.transom.delivery

import androidx.annotation.StringRes
import sh.transom.R

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

/** Whether this build on this phone can deliver notifications through Google Play services. */
sealed interface GooglePlay {
    /**
     * It can, through FCM, which [start] starts. The app starts it only once the person's choice is
     * Google Play services, so a phone set to the direct connection never starts Firebase.
     */
    class Available(start: () -> Push) : GooglePlay {
        val push: Push by lazy(start)
    }

    /** It can't, for the [reason] Settings shows under the option. */
    enum class Unavailable(@param:StringRes val reason: Int) : GooglePlay {
        NotInBuild(R.string.delivery_not_in_build),
        NotOnPhone(R.string.delivery_not_on_phone),
    }
}

/**
 * The delivery the phone uses when the person chose [chosen]: the choice, or the direct connection
 * while Google Play services is unavailable.
 */
fun GooglePlay.delivery(chosen: Delivery): Delivery =
    if (this is GooglePlay.Available) chosen else Delivery.Connection

/**
 * Whether FCM can reach this phone: it needs the Firebase project's [values] in the build and
 * Google Play services on the phone, which [playServices] says. [start] starts Firebase with the
 * values, once the app asks for the push, so a build or a phone without them never starts it.
 */
fun googlePlayFor(
    values: FirebaseValues?,
    playServices: () -> Boolean,
    start: (FirebaseValues) -> Push,
): GooglePlay =
    when {
        values == null -> GooglePlay.Unavailable.NotInBuild
        !playServices() -> GooglePlay.Unavailable.NotOnPhone
        else -> GooglePlay.Available { start(values) }
    }
