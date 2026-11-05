package sh.transom.exchange

import java.time.Instant
import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.Json

/**
 * An exchange as `GET /exchanges/{id}`, the list at `/@{name}/exchanges`, and its live feed give
 * it: the sender's [message] and, once there is one, the [reply].
 *
 * [from] and [to] are names with their `@`. [url] is absolute only when the server knows its public
 * URL, and otherwise a path such as `/exchanges/01JB…`. [read] is this person's private read state,
 * which the list gives only to the exchange's recipients and which `GET /exchanges/{id}` never
 * gives.
 */
@Serializable
data class Exchange(
    val id: String,
    val url: String,
    val state: State,
    val question: Boolean,
    val from: String,
    val to: String,
    @Serializable(with = InstantText::class) val created: Instant,
    val message: Message,
    val reply: Reply? = null,
    @Serializable(with = InstantText::class) val expires: Instant? = null,
    val read: Boolean? = null,
)

/** Where an exchange stands: `pending` → `open` → one of the four final states. */
@Serializable
enum class State {
    /** Held while the recipient decides whether to accept a sender it has not accepted yet. */
    @SerialName("pending") PENDING,
    @SerialName("open") OPEN,
    @SerialName("replied") REPLIED,
    @SerialName("expired") EXPIRED,
    @SerialName("withdrawn") WITHDRAWN,
    @SerialName("declined") DECLINED;

    /** Whether the exchange has ended, so nothing more will change it. */
    val isFinal: Boolean
        get() = this != PENDING && this != OPEN
}

/** The recipient's leg: a choice, text, or both, from [by] (a name with its `@`) [at] a time. */
@Serializable
data class Reply(
    val choice: String? = null,
    val text: String? = null,
    val by: String,
    @Serializable(with = InstantText::class) val at: Instant,
)

/**
 * How transom's JSON is read and written. Fields the server adds later are skipped, so an older app
 * keeps reading a newer server. A `null` where a field has a default reads as the default: the
 * server writes `"attachments": null` for a message without any, and an urgency added later reads
 * as [Urgency.NORMAL].
 */
val TransomJson = Json {
    ignoreUnknownKeys = true
    explicitNulls = false
    coerceInputValues = true
}

/** A timestamp as the server writes it: RFC 3339 in UTC, such as `2026-11-04T09:30:00.123Z`. */
internal object InstantText : KSerializer<Instant> {
    override val descriptor = PrimitiveSerialDescriptor("Instant", PrimitiveKind.STRING)

    override fun deserialize(decoder: Decoder): Instant = Instant.parse(decoder.decodeString())

    override fun serialize(encoder: Encoder, value: Instant) =
        encoder.encodeString(value.toString())
}
