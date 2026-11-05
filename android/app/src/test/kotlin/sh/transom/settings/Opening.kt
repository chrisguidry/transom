package sh.transom.settings

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import sh.transom.signin.awaitHeading

/** Opens Settings from the top bar's menu. */
fun ComposeTestRule.openSettings() {
    onNodeWithContentDescription("Menu").performClick()
    onNodeWithText("Settings").performClick()
    awaitHeading("Settings")
}

/** Opens [link], one of Settings' links to another screen, from Settings. */
fun ComposeTestRule.openFromSettings(link: String, heading: String) {
    openSettings()
    onNodeWithText(link).performScrollTo().performClick()
    awaitHeading(heading)
}

/**
 * Presses the button [text], scrolled to first, where the same words may also label something that
 * is not a button.
 */
fun ComposeTestRule.press(text: String) {
    onNode(hasText(text) and hasClickAction()).performScrollTo().performClick()
}
