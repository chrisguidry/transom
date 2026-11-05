package sh.transom.exchange

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import sh.transom.R
import sh.transom.theme.ActionButton
import sh.transom.theme.BodyLineHeight
import sh.transom.theme.ButtonKind
import sh.transom.theme.MetaText

/** A reply as the exchange shows it: who, when, the chosen choice's label, and the text. */
class ShownReply(val by: String, val time: String, val choice: String?, val text: String?)

/**
 * The reply, as the web's `.reply`: a rule of the accent beside who replied and when, the choice in
 * bold, and the text.
 */
@Composable
fun ReplySection(reply: ShownReply, modifier: Modifier = Modifier) {
    Row(modifier.height(IntrinsicSize.Min)) {
        Box(Modifier.width(3.dp).fillMaxHeight().background(MaterialTheme.colorScheme.primary))
        Column(Modifier.padding(start = 16.dp, top = 4.dp, bottom = 4.dp)) {
            MetaText(AnnotatedString(stringResource(R.string.replied, reply.by, reply.time)))
            reply.choice?.let {
                Text(
                    it,
                    Modifier.padding(vertical = 4.dp),
                    fontWeight = FontWeight.SemiBold,
                    lineHeight = BodyLineHeight,
                )
            }
            reply.text?.let { Text(it, lineHeight = BodyLineHeight) }
        }
    }
}

/**
 * The answer, as the web's `form.answer`: a button for each of the [message]'s choices, which
 * replies with that choice and whatever is in the text box, each choice's description, the prompt
 * over the text box, and Reply, which sends the text with no choice. [onReply] takes a choice's
 * key, or `null`, and the text, or `null` when the box is empty.
 */
@Composable
fun AnswerForm(
    message: Message,
    markdown: Markdown,
    onReply: (String?, String?) -> Unit,
    modifier: Modifier = Modifier,
) {
    var text by rememberSaveable { mutableStateOf("") }
    val note = { text.trim().ifEmpty { null } }
    val focus = LocalFocusManager.current
    // The reply takes the form's place at once, and clearing the focus first puts the keyboard
    // away, which would otherwise stay up over the reply with nothing to type into.
    val send = { choice: String? ->
        focus.clearFocus()
        onReply(choice, note())
    }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (message.choices.isNotEmpty()) {
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                message.choices.forEach { ActionButton(it.label, { send(it.key) }) }
            }
            message.choices
                .filter { !it.description.isNullOrEmpty() }
                .forEach { Description(it, markdown) }
        }
        val prompt = message.prompt?.takeIf { it.isNotEmpty() } ?: stringResource(R.string.prompt)
        Text(prompt, color = MaterialTheme.colorScheme.onSurfaceVariant)
        ReplyBox(text, { text = it }, prompt)
        // Any message takes free text, so Reply is always here to send it with no choice. Beside
        // choices it is quiet, so the choices stay the main buttons.
        ActionButton(
            stringResource(R.string.reply),
            { send(null) },
            kind = if (message.choices.isEmpty()) ButtonKind.Primary else ButtonKind.Quiet,
            enabled = note() != null,
        )
    }
}

/**
 * A choice's description, as the web's `.description`: the choice's label in bold, then its
 * Markdown, the first paragraph on the same line.
 */
@Composable
private fun Description(choice: Choice, markdown: Markdown) {
    val blocks = markdown.body(choice.description.orEmpty()).blocks
    val first = blocks.firstOrNull() as? Block.Paragraph
    val labelled = buildAnnotatedString {
        withStyle(Bold) { append(choice.label) }
        append(" ")
        first?.let { append(it.text) }
    }
    MarkdownBlocks(
        listOf(Block.Paragraph(labelled)) + blocks.drop(if (first == null) 0 else 1),
        picture = {},
        spacing = 8.dp,
    )
}

/** The web's `textarea`: three lines tall, ruled, with the accent's outline while focused. */
@Composable
private fun ReplyBox(text: String, onChange: (String) -> Unit, label: String) {
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val colors = MaterialTheme.colorScheme
    BasicTextField(
        value = text,
        onValueChange = onChange,
        modifier =
            Modifier.fillMaxWidth()
                .semantics { contentDescription = label }
                .border(
                    if (focused) 2.dp else 1.dp,
                    if (focused) colors.primary else colors.outline,
                    RoundedCornerShape(4.dp),
                )
                .padding(horizontal = 10.dp, vertical = 8.dp),
        textStyle =
            TextStyle(color = colors.onSurface, fontSize = 16.sp, lineHeight = BodyLineHeight),
        cursorBrush = SolidColor(colors.primary),
        minLines = 3,
        interactionSource = interaction,
    )
}
