package sh.transom.sync

import java.io.IOException

/**
 * Replies and read marks this person made on the phone, kept until the server has them. Each is
 * applied to the [store]'s inbox the moment it is queued, and [wake] asks for [deliver] to run once
 * the phone is online. A reply the server did not use, because someone else's came first, is kept
 * after it is sent, until the person has seen that it was not used.
 */
class Outbox(private val store: Store, private val wake: () -> Unit) {
    private val queue = store.database.queue()
    private val lost = store.database.lost()

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
     * Queues marking each of [ids] read, as a swipe on a row and Mark all read do. A row says when
     * this phone's reply to its exchange was not used, so marking it read forgets that reply.
     */
    suspend fun markAll(ids: List<String>) {
        queue.putReads(ids.map { QueuedRead(it, read = true) })
        ids.chunked(MOST_READS).forEach { lost.forget(it) }
        wake()
    }

    /**
     * Sends everything queued as [name], with [client], and settles each on the server's answer.
     * Returns `false` when something has to be sent again later, because the server could not be
     * reached or failed for now. Sending a reply twice is safe, since the server takes the same
     * reply from the same person again. When the server has ended the session, it throws
     * [SignedOut] and leaves everything queued: signing out decides what becomes of the queue.
     */
    suspend fun deliver(client: Client, name: String): Boolean {
        var delivered = true
        for (reply in queue.replies()) {
            delivered =
                send({ queue.sentReply(reply.exchange, reply.token) }) {
                    settle(client, name, reply)
                } && delivered
        }
        // Every exchange marked read goes in one request, however many there are, as after Mark all
        // read, and the server wakes each of this person's devices once for all of them. Marking
        // one unread has a request of its own.
        val (reads, unreads) = queue.reads().partition { it.read }
        for (batch in reads.chunked(MOST_READS)) {
            delivered =
                send({ batch.forEach { queue.sentRead(it.exchange, it.token) } }) {
                    client.markRead(name, batch.map { it.exchange })
                    batch.forEach { store.database.copies().setRead(it.exchange, true) }
                } && delivered
        }
        for (mark in unreads) {
            delivered =
                send({ queue.sentRead(mark.exchange, mark.token) }) {
                    client.mark(mark.exchange, name, read = false)
                    store.database.copies().setRead(mark.exchange, false)
                } && delivered
        }
        return delivered
    }

    /**
     * Copies the server's answer to [reply]: the exchange with it, or, when someone else replied
     * first, the exchange with theirs, and [reply] as the one that was not used.
     */
    private suspend fun settle(client: Client, name: String, reply: QueuedReply) {
        val copies = store.database.copies()
        when (val replied = client.reply(reply.exchange, reply.choice, reply.text)) {
            is Replied.Accepted -> copies.save(replied.exchange, name)
            is Replied.Lost -> {
                copies.save(client.exchange(reply.exchange), name)
                lost.put(LostReply(reply.exchange, reply.choice, reply.text))
            }
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

/**
 * How many exchanges one request marks read at most, so a request stays a few kilobytes. Replies
 * that were not used are forgotten as many at a time, under the 999 values that Android 10's SQLite
 * takes in one statement.
 */
private const val MOST_READS = 200
