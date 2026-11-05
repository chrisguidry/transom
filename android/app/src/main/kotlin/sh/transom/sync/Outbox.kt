package sh.transom.sync

import java.io.IOException

/**
 * Replies and read marks this person made on the phone, kept until the server has them. Each is
 * applied to the [store]'s inbox the moment it is queued, and [wake] asks for [deliver] to run once
 * the phone is online.
 */
class Outbox(private val store: Store, private val wake: () -> Unit) {
    private val queue = store.database.queue()

    /**
     * Queues a reply to exchange [id]: [choice], a choice's key, and [text], at least one of them.
     */
    suspend fun reply(id: String, choice: String? = null, text: String? = null) {
        queue.put(QueuedReply(id, choice, text))
        wake()
    }

    /** Queues marking exchange [id] [read] or unread. */
    suspend fun mark(id: String, read: Boolean) {
        queue.put(QueuedRead(id, read))
        wake()
    }

    /**
     * Sends everything queued as [name], with [client], and settles each on the server's answer.
     * Returns `false` when something has to be sent again later, because the server could not be
     * reached or failed for now. Sending a reply twice is safe, since the server takes the same
     * reply from the same person again.
     */
    suspend fun deliver(client: Client, name: String): Boolean {
        var delivered = true
        for (reply in queue.replies()) {
            delivered =
                send({ queue.sentReply(reply.exchange, reply.token) }) {
                    settle(client, name, reply)
                } && delivered
        }
        for (mark in queue.reads()) {
            delivered =
                send({ queue.sentRead(mark.exchange, mark.token) }) {
                    client.mark(mark.exchange, name, mark.read)
                    store.database.copies().setRead(mark.exchange, mark.read)
                } && delivered
        }
        return delivered
    }

    /**
     * Copies the server's answer to [reply]: the exchange with it, or, when someone else replied
     * first, the exchange with theirs.
     */
    private suspend fun settle(client: Client, name: String, reply: QueuedReply) {
        val copies = store.database.copies()
        when (val replied = client.reply(reply.exchange, reply.choice, reply.text)) {
            is Replied.Accepted -> copies.save(replied.exchange, name)
            is Replied.Lost -> copies.save(client.exchange(reply.exchange), name)
        }
    }

    /**
     * Runs [request], and dequeues what it sent unless it must be sent again. A refusal that would
     * come again, such as a reply to an exchange that expired, is dequeued too: the copy then shows
     * the exchange as the server last described it, and the feed brings what changed.
     */
    private suspend fun send(dequeue: suspend () -> Unit, request: suspend () -> Unit): Boolean =
        try {
            request()
            dequeue()
            true
        } catch (_: IOException) {
            false
        } catch (refused: Refused) {
            if (!refused.transient) dequeue()
            !refused.transient
        }
}
