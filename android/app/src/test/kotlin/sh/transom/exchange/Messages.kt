package sh.transom.exchange

import java.awt.image.BufferedImage
import java.io.ByteArrayOutputStream
import java.util.Base64
import javax.imageio.ImageIO
import kotlinx.serialization.json.JsonObjectBuilder
import kotlinx.serialization.json.addJsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray

/**
 * A message as the JSON `POST /@{name}` takes, for the tests to send as curl would: [choices] are
 * labels with optional descriptions, and [attachments] are names with their bytes.
 */
fun message(
    summary: String,
    body: String? = null,
    prompt: String? = null,
    choices: List<Pair<String, String?>> = emptyList(),
    links: List<Pair<String, String?>> = emptyList(),
    attachments: List<Pair<String, ByteArray>> = emptyList(),
): String = buildJsonObject {
    put("summary", summary)
    body?.let { put("body", it) }
    prompt?.let { put("prompt", it) }
    list("choices", choices) { (label, description) ->
        put("label", label)
        description?.let { put("description", it) }
    }
    list("link", links) { (href, rel) ->
        put("href", href)
        rel?.let { put("rel", it) }
    }
    list("attachments", attachments) { (name, bytes) ->
        put("name", name)
        put("data", Base64.getEncoder().encodeToString(bytes))
    }
}
    .toString()

private fun <T> JsonObjectBuilder.list(
    name: String,
    items: List<T>,
    item: JsonObjectBuilder.(T) -> Unit,
) {
    if (items.isNotEmpty()) putJsonArray(name) { items.forEach { addJsonObject { item(it) } } }
}

/** A PNG of [width] by [height] pixels. */
fun png(width: Int, height: Int): ByteArray =
    ByteArrayOutputStream()
        .also { ImageIO.write(BufferedImage(width, height, BufferedImage.TYPE_INT_RGB), "png", it) }
        .toByteArray()
