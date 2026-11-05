package sh.transom.notifications

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import sh.transom.exchange.Attachment
import sh.transom.exchange.Message
import sh.transom.signin.app
import sh.transom.sync.Client
import sh.transom.sync.MemoryStore
import sh.transom.sync.Outbox
import sh.transom.sync.exchange
import sh.transom.testserver.NOWHERE

/** A notification whose image cannot be fetched, as while the server cannot be reached. */
@RunWith(RobolectricTestRunner::class)
class UnfetchedImageTest {
    @get:Rule val memory = MemoryStore()

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    @After fun stop() = scope.cancel()

    @Test
    fun showsTheTextAlone() {
        val store = memory.store
        val notifier = Notifier(app, store, Outbox(store) {}, "bob", Client(NOWHERE, "token"))
        scope.launch { notifier.follow() }
        val poster =
            Attachment("poster.png", "image/png", 70, "/exchanges/import/attachments/poster.png")
        val message = Message(summary = "Severance S02E04 is ready", attachments = listOf(poster))
        val arrived = System.currentTimeMillis() / 1000 + 60
        runBlocking {
            store.database
                .copies()
                .replace(
                    listOf(
                        exchange("import", question = false, created = arrived)
                            .copy(message = message)
                    )
                )
        }
        assertEquals(listOf("@alice: Severance S02E04 is ready"), awaitPosted("import").messages)
    }
}
