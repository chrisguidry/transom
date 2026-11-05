package sh.transom.sync

import java.util.concurrent.TimeUnit
import kotlin.time.Duration
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.buffer
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.serialization.Serializable
import okhttp3.OkHttpClient
import okhttp3.Response
import okhttp3.sse.EventSource
import okhttp3.sse.EventSourceListener
import okhttp3.sse.EventSources
import sh.transom.exchange.Exchange
import sh.transom.exchange.TransomJson

/** What the live feed at `/@{name}/exchanges` says. */
sealed interface FeedEvent {
    /**
     * The feed is connected. It sends only what changes from here on, so whatever changed before
     * this has to be read from the list.
     */
    data object Opened : FeedEvent

    /** An exchange in the list changed, or its read state did; this is all of it, with `read`. */
    data class Changed(val exchange: Exchange) : FeedEvent

    /** The exchange was deleted with the group that received it. */
    data class Gone(val id: String) : FeedEvent
}

/**
 * Follows [name]'s live feed for as long as it is collected, completing when the server closes it
 * and throwing when the connection fails. It never reconnects: [Sync.follow] does that.
 *
 * The server subscribes before it answers, so a change made after [FeedEvent.Opened] arrives is
 * never missed. Events wait in an unbounded buffer while the collector is busy, so a slow refresh
 * loses none.
 */
fun Client.feed(name: String): Flow<FeedEvent> = callbackFlow {
    val request = request(url("@$name", "exchanges")).header("Accept", "text/event-stream")
    val listener =
        object : EventSourceListener() {
            override fun onOpen(eventSource: EventSource, response: Response) {
                trySend(Raw(OPENED, ""))
            }

            override fun onEvent(
                eventSource: EventSource,
                id: String?,
                type: String?,
                data: String,
            ) {
                trySend(Raw(type.orEmpty(), data))
            }

            override fun onClosed(eventSource: EventSource) {
                channel.close()
            }

            override fun onFailure(
                eventSource: EventSource,
                t: Throwable?,
                response: Response?,
            ) {
                channel.close(t ?: Refused.of(response!!.code, response.body.string()))
            }
        }
    val source =
        EventSources.createFactory(listening(http)).newEventSource(request.build(), listener)
    awaitClose { source.cancel() }
}
    .buffer(Channel.UNLIMITED)
    .mapNotNull { decode(it.type, it.data) }

/**
 * The event a feed's [type] and [data] make. The server ends a feed whose check failed with an
 * `error` event, and any other type is one this app does not know, so both are skipped.
 */
internal fun decode(type: String, data: String): FeedEvent? =
    when (type) {
        OPENED -> FeedEvent.Opened
        "exchange" -> FeedEvent.Changed(TransomJson.decodeFromString(data))
        "gone" -> FeedEvent.Gone(TransomJson.decodeFromString<GoneId>(data).id)
        else -> null
    }

/**
 * The server sends a comment every 15 seconds on a feed with nothing to say, so a read that waits
 * longer than [FEED_SILENCE] has lost its connection, though no error said so.
 */
private fun listening(http: OkHttpClient) =
    http.newBuilder().readTimeout(FEED_SILENCE.inWholeSeconds, TimeUnit.SECONDS).build()

private val FEED_SILENCE = 45.seconds

/**
 * The type the connection's opening is passed on as. A stream's reader drops the space after
 * `event:`, so no event the server sends has a type that starts with one.
 */
private const val OPENED = " opened"

private class Raw(val type: String, val data: String)

@Serializable private class GoneId(val id: String)

/**
 * How long to wait before reconnecting after [failures] connections in a row have failed: [first],
 * doubling each time, up to [most].
 */
data class Backoff(val first: Duration = 1.seconds, val most: Duration = 1.minutes) {
    fun after(failures: Int): Duration =
        minOf(first * (1L shl failures.coerceIn(0, 30)).toDouble(), most)
}
