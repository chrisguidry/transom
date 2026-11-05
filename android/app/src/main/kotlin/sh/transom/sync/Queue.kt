package sh.transom.sync

import androidx.room.Dao
import androidx.room.Entity
import androidx.room.PrimaryKey
import androidx.room.Query
import androidx.room.Upsert
import java.util.UUID

/**
 * A reply waiting to be sent. Each exchange has one at most: a later one replaces it. [token] tells
 * this one from a reply queued while it was being sent, which must not be dequeued with it.
 */
@Entity(tableName = "queued_replies")
internal data class QueuedReply(
    @PrimaryKey val exchange: String,
    val choice: String?,
    val text: String?,
    val token: String = UUID.randomUUID().toString(),
)

/** A read mark waiting to be sent, one per exchange, the latest replacing the one before. */
@Entity(tableName = "queued_reads")
internal data class QueuedRead(
    @PrimaryKey val exchange: String,
    val read: Boolean,
    val token: String = UUID.randomUUID().toString(),
)

@Dao
internal interface QueueDao {
    @Upsert suspend fun put(reply: QueuedReply)

    @Upsert suspend fun put(read: QueuedRead)

    @Upsert suspend fun putReads(reads: List<QueuedRead>)

    @Query("SELECT * FROM queued_replies") suspend fun replies(): List<QueuedReply>

    @Query("SELECT * FROM queued_reads") suspend fun reads(): List<QueuedRead>

    @Query("DELETE FROM queued_replies") suspend fun clearReplies()

    @Query("DELETE FROM queued_reads") suspend fun clearReads()

    @Query("DELETE FROM queued_replies WHERE exchange = :exchange AND token = :token")
    suspend fun sentReply(exchange: String, token: String)

    @Query("DELETE FROM queued_reads WHERE exchange = :exchange AND token = :token")
    suspend fun sentRead(exchange: String, token: String)
}
