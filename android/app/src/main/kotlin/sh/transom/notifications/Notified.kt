package sh.transom.notifications

import androidx.room.ColumnInfo
import androidx.room.Dao
import androidx.room.Entity
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.PrimaryKey
import androidx.room.Query
import androidx.room.Transaction
import androidx.room.Upsert

/**
 * An exchange this phone has posted a notification for, kept until the notification is cancelled,
 * so a notification posted again after the process or the phone restarts makes no sound a second
 * time. [answer] is what the person replied from the notification, which it shows from "Me" until
 * the server settles the reply.
 *
 * [tag] is the notification's tag when it is not the exchange's own id: a message that replaced an
 * earlier one with the same key took over the earlier one's notification. [repeats] is how many of
 * an urgent exchange's repeats the notification has alerted for, so a repeat that arrives sounds
 * once, and one heard before a restart does not sound again.
 */
@Entity(tableName = "notified")
internal data class Notified(
    @PrimaryKey val exchange: String,
    val answer: String? = null,
    val tag: String? = null,
    @ColumnInfo(defaultValue = "0") val repeats: Int = 0,
)

/** The tag the notification is posted under. */
internal val Notified.posted: String
    get() = tag ?: exchange

/**
 * When this phone began notifying for the session it is signed in with, in epoch milliseconds. A
 * one-way notification that arrived before then is no news, so only one that arrived since is
 * posted. The table has one row at most.
 */
@Entity(tableName = "notifying")
internal data class Notifying(@PrimaryKey val row: Int = 0, val since: Long)

@Dao
internal abstract class NotifiedDao {
    @Query("SELECT * FROM notified") abstract suspend fun all(): List<Notified>

    @Query("SELECT * FROM notified WHERE exchange = :exchange")
    abstract suspend fun find(exchange: String): Notified?

    @Upsert abstract suspend fun put(notified: Notified)

    @Query("DELETE FROM notified WHERE exchange = :exchange")
    abstract suspend fun drop(exchange: String)

    @Query("DELETE FROM notified") abstract suspend fun clear()

    @Insert(onConflict = OnConflictStrategy.IGNORE) abstract suspend fun keep(since: Notifying)

    @Query("SELECT since FROM notifying") abstract suspend fun kept(): Long

    @Query("DELETE FROM notifying") abstract suspend fun clearSince()

    /** When notifying began, which is [now] the first time it is asked for. */
    @Transaction
    open suspend fun since(now: Long): Long {
        keep(Notifying(since = now))
        return kept()
    }
}
