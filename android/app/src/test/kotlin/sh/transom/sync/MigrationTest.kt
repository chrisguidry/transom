package sh.transom.sync

import android.content.Context
import android.database.sqlite.SQLiteDatabase
import androidx.test.core.app.ApplicationProvider
import java.io.File
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** A phone that has the database of the version before opens it as this one. */
@RunWith(RobolectricTestRunner::class)
class MigrationTest {
    private val context: Context = ApplicationProvider.getApplicationContext()

    /**
     * Makes the phone's database as [version] of the app made it, from the schema Room wrote for
     * that version, holding a reply to the exchange `asked` that waits to be sent.
     */
    private fun databaseOf(version: Int) {
        val schema =
            Json.parseToJsonElement(File(SCHEMAS, "$version.json").readText())
                .jsonObject
                .getValue("database")
                .jsonObject
        val file = context.getDatabasePath("sync.db")
        checkNotNull(file.parentFile).mkdirs()
        SQLiteDatabase.openOrCreateDatabase(file, null).use { database ->
            for (entity in schema.getValue("entities").jsonArray) {
                val table = entity.jsonObject.getValue("tableName").jsonPrimitive.content
                val create = entity.jsonObject.getValue("createSql").jsonPrimitive.content
                database.execSQL(create.replace("\${TABLE_NAME}", table))
            }
            for (query in schema.getValue("setupQueries").jsonArray) {
                database.execSQL(query.jsonPrimitive.content)
            }
            database.execSQL("INSERT INTO queued_replies VALUES ('asked', 'hold', NULL, 'token')")
            database.version = version
        }
    }

    @Test
    fun version3KeepsWhatItQueuedAndTakesAReplyThatWasNotUsed() = runBlocking {
        databaseOf(3)
        val database = SyncDatabase.open(context)
        database.copies().replace(listOf(exchange("asked")))
        database.lost().put(LostReply("asked", "deploy", null))
        val entry = Store(database).exchange("asked").first()
        assertEquals(
            listOf(Sending("hold", null), Sending("deploy", null)),
            listOf(entry?.sending, entry?.lost),
        )
        database.close()
    }

    private companion object {
        val SCHEMAS = File(System.getProperty("transom.schemas")!!)
    }
}
