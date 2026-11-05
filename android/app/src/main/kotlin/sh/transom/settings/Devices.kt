package sh.transom.settings

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import sh.transom.exchange.TransomJson
import sh.transom.sync.Client

/**
 * One of an identity's devices: its [id], the [name] it registered with, and how transom wakes it,
 * as `{"web_push": …}` for a browser or `{"connection": true}` for a phone that holds the feed
 * open.
 */
@Serializable
data class Device(val id: String, val name: String, val wake: JsonObject) {
    /** Whether a browser registered it, to be woken by Web Push. */
    val browser: Boolean
        get() = "web_push" in wake
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
