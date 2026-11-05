package sh.transom.exchange

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * The sender's leg of an exchange, which never changes once sent.
 *
 * A message with [choices] or a [prompt] is a question; an empty prompt still makes one, with a
 * text box and no label. [metadata] is the sender's own data: the server gives it only to the
 * sender, so a recipient always sees it empty.
 */
@Serializable
data class Message(
    val summary: String? = null,
    val body: String? = null,
    val choices: List<Choice> = emptyList(),
    val prompt: String? = null,
    /** How long the message stays open, in seconds, as the sender gave it. */
    val ttl: Long? = null,
    @SerialName("link") val links: List<Link> = emptyList(),
    val urgency: Urgency = Urgency.NORMAL,
    val metadata: Map<String, String> = emptyMap(),
    val attachments: List<Attachment> = emptyList(),
    /**
     * The sender's name for what the message is about, such as an alert's. A later message from the
     * same sender to the same recipient with the same key replaces this one while it is open.
     */
    val key: String? = null,
)

/** One of a question's choices, with an optional Markdown [description]. */
@Serializable
data class Choice(val label: String, val description: String? = null, val id: String? = null) {
    /** What a reply names this choice by: its id, or its label when it has none. */
    val key: String
        get() = id ?: label
}

/**
 * A link that goes with the message, with its registered relation type, such as `related`, and the
 * [title] that labels it, such as "View run".
 */
@Serializable data class Link(val href: String, val rel: String? = null, val title: String? = null)

/**
 * An image sent with the message. The body places one inline by naming it, as `![the
 * graph](graph.png)`, and the server serves it at [url] behind the same sign-in as the message.
 */
@Serializable
data class Attachment(
    val name: String,
    @SerialName("type") val contentType: String,
    val size: Long,
    val url: String,
)

/**
 * How urgently to deliver a message, with the values of Web Push's `Urgency` (RFC 8030), and
 * `urgent` above them, for an alert that should reach a person who is asleep or in a meeting. The
 * levels order from least to most urgent, as they are declared.
 */
@Serializable
enum class Urgency {
    @SerialName("very-low") VERY_LOW,
    @SerialName("low") LOW,
    @SerialName("normal") NORMAL,
    @SerialName("high") HIGH,
    @SerialName("urgent") URGENT,
}
