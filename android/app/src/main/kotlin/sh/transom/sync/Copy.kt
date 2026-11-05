package sh.transom.sync

import androidx.room.Dao
import androidx.room.Entity
import androidx.room.PrimaryKey
import androidx.room.Query
import androidx.room.Transaction
import androidx.room.Upsert
import kotlinx.coroutines.flow.Flow
import sh.transom.exchange.Exchange

/**
 * One exchange as the server last described it. [created] orders the inbox as the server does, and
 * [read] is `null` for an exchange this person is not a recipient of, such as one they sent.
 */
@Entity(tableName = "exchanges")
internal data class Copied(
    @PrimaryKey val id: String,
    val exchange: Exchange,
    val created: Long,
    val read: Boolean?,
) {
    constructor(
        exchange: Exchange,
        read: Boolean?,
    ) : this(exchange.id, exchange, exchange.created.toEpochMilli(), read)
}

/** A copied exchange with what is queued for it: a reply being sent, and a read mark. */
internal data class Row(
    val exchange: Exchange,
    val read: Boolean?,
    val sending: Boolean,
    val choice: String?,
    val text: String?,
    val marked: Boolean?,
)

private const val ROWS =
    """
    SELECT e.exchange, e.read, r.exchange IS NOT NULL AS sending, r.choice, r.text,
        m.read AS marked
    FROM exchanges e
    LEFT JOIN queued_replies r ON r.exchange = e.id
    LEFT JOIN queued_reads m ON m.exchange = e.id
    """

@Dao
internal abstract class CopyDao {
    /** Every copied exchange, newest first, ordered as the server lists them. */
    @Query("$ROWS ORDER BY e.created DESC, e.id DESC") abstract fun rows(): Flow<List<Row>>

    @Query("$ROWS WHERE e.id = :id") abstract fun row(id: String): Flow<Row?>

    @Query("SELECT * FROM exchanges WHERE id = :id") abstract suspend fun find(id: String): Copied?

    @Upsert abstract suspend fun put(copied: Copied)

    @Query("DELETE FROM exchanges WHERE id = :id") abstract suspend fun drop(id: String)

    @Query("DELETE FROM exchanges") abstract suspend fun clear()

    @Query("DELETE FROM exchanges WHERE id NOT IN (:ids)")
    abstract suspend fun keepOnly(ids: List<String>)

    /** Sets the read state of an exchange this person is a recipient of. */
    @Query("UPDATE exchanges SET read = :read WHERE id = :id AND read IS NOT NULL")
    abstract suspend fun setRead(id: String, read: Boolean)

    /**
     * Makes the copy what the server listed: [listed] replaces what was there, and an exchange the
     * server no longer lists is dropped.
     */
    @Transaction
    open suspend fun replace(listed: List<Exchange>) {
        keepOnly(listed.map { it.id })
        for (exchange in listed) {
            if (!find(exchange.id).endedBefore(exchange)) put(Copied(exchange, exchange.read))
        }
    }

    /**
     * Saves one exchange the server described outside the list, as [name] sees it. The feed gives
     * `read` even to the sender, and `GET /exchanges/{id}` gives it to no one, so whether this
     * person has a read state comes from the list when the copy has the exchange already, and
     * otherwise from whether it was sent to someone else.
     */
    @Transaction
    open suspend fun save(exchange: Exchange, name: String) {
        val kept = find(exchange.id)
        if (kept.endedBefore(exchange)) return
        val read =
            if (kept != null) kept.read?.let { exchange.read ?: it }
            else exchange.read.takeIf { exchange.from != "@$name" || exchange.to == "@$name" }
        put(Copied(exchange, read))
    }
}

/**
 * Whether the copy has [exchange] as ended while the server describes it as still open or pending.
 * An exchange that ended never opens again, so that description was made before it ended and
 * arrived late, and the copy keeps what it has.
 */
private fun Copied?.endedBefore(exchange: Exchange): Boolean =
    this?.exchange?.state?.isFinal == true && !exchange.state.isFinal
