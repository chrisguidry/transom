package sh.transom.notifications

import org.junit.Assert.assertEquals
import org.junit.Test
import sh.transom.exchange.Choice
import sh.transom.exchange.Message

/** A notification offers chips only when every choice fits in Android's one row of them. */
class ChipsTest {
    private fun chipsOf(vararg labels: String) =
        Message(choices = labels.map { Choice(it) }).chips.map { it.label }

    @Test
    fun threeShortChoicesAreChips() =
        assertEquals(listOf("Deploy", "Hold", "Roll back"), chipsOf("Deploy", "Hold", "Roll back"))

    @Test fun aQuestionWithoutChoicesHasNoChips() = assertEquals(emptyList<String>(), chipsOf())

    @Test
    fun fourChoicesAreNotChips() =
        assertEquals(emptyList<String>(), chipsOf("One", "Two", "Three", "Four"))

    @Test
    fun choicesTooLongToFitAreNotChips() =
        assertEquals(
            emptyList<String>(),
            chipsOf("Deploy to production", "Hold until Monday", "Roll back"),
        )

    @Test
    fun labelsUpToTheLimitAreChips() =
        assertEquals(3, chipsOf("a".repeat(10), "b".repeat(10), "c".repeat(10)).size)

    @Test
    fun twoChoicesWithOneLabelAreNotChips() =
        assertEquals(
            emptyList<String>(),
            Message(choices = listOf(Choice("Yes", id = "a"), Choice("Yes", id = "b"))).chips,
        )
}
