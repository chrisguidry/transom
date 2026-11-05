package sh.transom.sync

import android.content.Context
import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase
import androidx.room.TypeConverter
import androidx.room.TypeConverters
import sh.transom.exchange.Exchange
import sh.transom.exchange.TransomJson

/**
 * The phone's copy of the server: the exchanges as it last described them, what is queued to send
 * to it, and this phone's device id.
 */
@Database(
    entities = [Copied::class, QueuedReply::class, QueuedRead::class, ThisDevice::class],
    version = 1,
)
@TypeConverters(ExchangeText::class)
abstract class SyncDatabase : RoomDatabase() {
    internal abstract fun copies(): CopyDao

    internal abstract fun queue(): QueueDao

    internal abstract fun device(): DeviceDao

    companion object {
        /** The database on this phone's storage, which outlives the app's process. */
        fun open(context: Context): SyncDatabase =
            Room.databaseBuilder(context, SyncDatabase::class.java, "sync.db").build()
    }
}

/**
 * An exchange is kept as the JSON the app reads, beside the columns the inbox queries on, so a
 * field the inbox never filters by needs no column of its own.
 */
internal class ExchangeText {
    @TypeConverter fun write(exchange: Exchange): String = TransomJson.encodeToString(exchange)

    @TypeConverter fun read(text: String): Exchange = TransomJson.decodeFromString(text)
}
