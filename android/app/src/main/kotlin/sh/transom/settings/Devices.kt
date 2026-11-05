package sh.transom.settings

import androidx.annotation.StringRes
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import sh.transom.R
import sh.transom.exchange.TransomJson
import sh.transom.sync.Client

/**
 * One of an identity's devices: its [id], the [name] it registered with, and how transom wakes it,
 * as `{"web_push": …}` for a browser, `{"fcm": …}` for a phone that Google Play services delivers
 * to, or `{"connection": true}` for a phone that holds the feed open.
 */
@Serializable
data class Device(val id: String, val name: String, val wake: JsonObject) {
    /** How notifications reach it, as the devices list says. */
    val deliveredBy: DeliveredBy
        get() =
            when {
                "web_push" in wake -> DeliveredBy.WebPush
                "fcm" in wake -> DeliveredBy.GooglePlay
                else -> DeliveredBy.Connection
            }
}

/** How notifications reach a device, with the [label] the devices list gives it. */
enum class DeliveredBy(@param:StringRes val label: Int) {
    WebPush(R.string.devices_web_push),
    GooglePlay(R.string.devices_google_play),
    Connection(R.string.devices_connection),
}

/** [name]'s devices, which only [name] may list. */
suspend fun Client.devices(name: String): List<Device> =
    TransomJson.decodeFromString<Listing>(send(request(url("@$name", "devices")))).devices

/**
 * Forgets [name]'s device [id]. Forgetting a phone also ends the session it registered with, so the
 * phone is signed out.
 */
suspend fun Client.forget(name: String, id: String) {
    send(request(url("@$name", "devices", id)).delete())
}

@Serializable private class Listing(val devices: List<Device>)
