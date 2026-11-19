package sh.transom.sync

import androidx.room.Dao
import androidx.room.Entity
import androidx.room.PrimaryKey
import androidx.room.Query
import androidx.room.Upsert

/**
 * A reply this phone sent that the server did not use, because someone else's came first: what it
 * said, kept until the person has seen that it was not used. Each exchange has one at most.
 */
@Entity(tableName = "lost_replies")
internal data class LostReply(
    @PrimaryKey val exchange: String,
    val choice: String?,
    val text: String?,
)

@Dao
internal interface LostDao {
    @Upsert suspend fun put(reply: LostReply)

    @Query("DELETE FROM lost_replies WHERE exchange IN (:exchanges)")
    suspend fun forget(exchanges: List<String>)

    @Query("DELETE FROM lost_replies") suspend fun clear()
}
