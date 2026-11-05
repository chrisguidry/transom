package sh.transom.wake

import android.content.Context
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.GoogleApiAvailability
import com.google.android.gms.tasks.Task
import com.google.firebase.FirebaseApp
import com.google.firebase.FirebaseOptions
import com.google.firebase.installations.FirebaseInstallations
import com.google.firebase.messaging.FirebaseMessaging
import kotlin.coroutines.resume
import kotlinx.coroutines.suspendCancellableCoroutine
import sh.transom.BuildConfig

/**
 * Code that runs only with Google Play services and a Firebase project, which the unit tests have
 * neither of under Robolectric, so coverage leaves it out. It is kept to the calls that reach
 * Firebase and Play services, and the choice around them runs in the tests through [pushFor] and
 * [Push].
 */
@Retention(AnnotationRetention.BINARY)
@Target(AnnotationTarget.CLASS, AnnotationTarget.FUNCTION)
annotation class FirebaseOnly

/** Firebase for the app's process, started from the values the app was built with. */
@FirebaseOnly
internal object Firebase {
    /**
     * FCM for this phone, started now, or `null` when the build has no Firebase project or the
     * phone has no Google Play services.
     */
    fun push(context: Context): Push? =
        pushFor(
            FirebaseValues.of(
                BuildConfig.FIREBASE_PROJECT_ID,
                BuildConfig.FIREBASE_APPLICATION_ID,
                BuildConfig.FIREBASE_API_KEY,
                BuildConfig.FIREBASE_SENDER_ID,
            ),
            playServices = {
                GoogleApiAvailability.getInstance().isGooglePlayServicesAvailable(context) ==
                    ConnectionResult.SUCCESS
            },
            start = { FirebasePush(context, it) },
        )
}

/**
 * FCM through the Firebase SDK, which starts the default `FirebaseApp` with [values] in place of
 * the `google-services.json` that the Google Services plugin would read.
 */
@FirebaseOnly
private class FirebasePush(context: Context, values: FirebaseValues) : Push {
    init {
        val options =
            FirebaseOptions.Builder()
                .setProjectId(values.projectId)
                .setApplicationId(values.applicationId)
                .setApiKey(values.apiKey)
                .setGcmSenderId(values.senderId)
                .build()
        FirebaseApp.initializeApp(context, options)
    }

    /**
     * The installation ID once FCM has registered it, which FCM sends messages to. FCM deprecated
     * its registration tokens for these, and its API takes one where it takes a token.
     */
    override suspend fun token(): String? {
        if (!FirebaseMessaging.getInstance().register().settled().isSuccessful) return null
        val id = FirebaseInstallations.getInstance().id.settled()
        return if (id.isSuccessful) id.result else null
    }

    private suspend fun <T> Task<T>.settled(): Task<T> = suspendCancellableCoroutine { asked ->
        addOnCompleteListener { asked.resume(it) }
    }
}
