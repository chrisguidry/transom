package sh.transom.sync

import android.content.Context
import androidx.room.AutoMigration
import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase
import androidx.room.TypeConverter
import androidx.room.TypeConverters
import sh.transom.exchange.Exchange
import sh.transom.exchange.TransomJson
import sh.transom.notifications.Notified
import sh.transom.notifications.NotifiedDao
import sh.transom.notifications.Notifying

/**
 * The phone's copy of the server: the exchanges as it last described them, what is queued to send
 * to it, this phone's device id, and the notifications it has posted.
 *
 * Version 2 adds the notifications' tables, and version 3 the tag and the repeats of each
 * notification, each made by Room's automatic migration from the schemas in `schemas/`.
 */
@Database(
    entities =
        [
            Copied::class,
            QueuedReply::class,
            QueuedRead::class,
            ThisDevice::class,
            Notified::class,
            Notifying::class,
        ],
    version = 3,
    autoMigrations = [AutoMigration(from = 1, to = 2), AutoMigration(from = 2, to = 3)],
)
@TypeConverters(ExchangeText::class)
abstract class SyncDatabase : RoomDatabase() {
    internal abstract fun copies(): CopyDao

    internal abstract fun queue(): QueueDao

    internal abstract fun device(): DeviceDao

    internal abstract fun notified(): NotifiedDao

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
