package sh.transom.sync

import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeout
import org.junit.rules.ExternalResource

/** A [Store] in an in-memory database of its own for each test, closed after it. */
class MemoryStore : ExternalResource() {
    lateinit var store: Store
        private set

    override fun before() {
        store =
            Store(
                Room.inMemoryDatabaseBuilder(
                        ApplicationProvider.getApplicationContext(),
                        SyncDatabase::class.java,
                    )
                    .build()
            )
    }

    override fun after() {
        store.database.close()
    }
}

/**
 * The first value of the flow that passes [condition], which must come within ten seconds. Tests
 * wait on what they expect, never for a fixed time.
 */
suspend fun <T> Flow<T>.awaiting(condition: (T) -> Boolean): T =
    withTimeout(10.seconds) { first(condition) }
