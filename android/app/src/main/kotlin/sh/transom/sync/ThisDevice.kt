package sh.transom.sync

import androidx.room.Dao
import androidx.room.Entity
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.PrimaryKey
import androidx.room.Query
import androidx.room.Transaction
import java.util.UUID

/**
 * The id this phone registers as a device under, made once and kept, so registering again replaces
 * the phone's device rather than adding another. The table has one row at most.
 */
@Entity(tableName = "this_device")
internal data class ThisDevice(@PrimaryKey val row: Int = 0, val id: String)

@Dao
internal abstract class DeviceDao {
    @Insert(onConflict = OnConflictStrategy.IGNORE) abstract suspend fun keep(device: ThisDevice)

    @Query("SELECT id FROM this_device") abstract suspend fun kept(): String

    /** This phone's device id, made the first time it is asked for. */
    @Transaction
    open suspend fun id(): String {
        keep(ThisDevice(id = UUID.randomUUID().toString()))
        return kept()
    }
}
