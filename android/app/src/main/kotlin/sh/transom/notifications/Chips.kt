package sh.transom.notifications

import sh.transom.exchange.Choice
import sh.transom.exchange.Message

/**
 * The choices a notification offers as the reply box's chips, or none.
 *
 * Android shows one row of chips under a notification, about three short labels on a phone, and
 * drops the ones that do not fit without a sign. A question offering some of its choices would read
 * as if those were all of them, so chips are offered only when every choice fits: at most
 * [MOST_CHIPS] choices, whose labels together are at most [MOST_CHIP_CHARACTERS] characters, and no
 * two with the same label, since a chip answers with its label alone. Otherwise the reply box takes
 * text only, and the choices are in the app.
 */
internal val Message.chips: List<Choice>
    get() {
        val labels = choices.map { it.label }
        val fit =
            choices.size <= MOST_CHIPS &&
                labels.sumOf { it.codePointCount(0, it.length) } <= MOST_CHIP_CHARACTERS &&
                labels.distinct().size == labels.size
        return if (fit) choices else emptyList()
    }

internal const val MOST_CHIPS = 3
internal const val MOST_CHIP_CHARACTERS = 30
