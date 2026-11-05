package sh.transom.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp

/** The web UI's line height for running text, from `body` in `transom.css`. */
val BodyLineHeight = 1.55.em

/** A page's heading, as the web UI's `h1`. */
@Composable
fun PageHeading(text: String, modifier: Modifier = Modifier) {
    Text(
        text = text,
        modifier = modifier.semantics { heading() },
        fontSize = 24.sp,
        fontWeight = FontWeight.SemiBold,
        lineHeight = 1.3.em,
    )
}

/** A section's label, as the web UI's `h2`: small, muted, and uppercase. */
@Composable
fun SectionLabel(text: String, modifier: Modifier = Modifier) {
    Text(
        text = text.uppercase(),
        modifier = modifier.semantics { heading() },
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        fontSize = 13.6.sp,
        fontWeight = FontWeight.SemiBold,
        letterSpacing = 0.04.em,
    )
}

/** Who, when, and where an exchange stands, as the web UI's `.meta`: small and muted. */
@Composable
fun MetaText(text: AnnotatedString, modifier: Modifier = Modifier) {
    Text(
        text = text,
        modifier = modifier,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        fontSize = 14.sp,
        lineHeight = BodyLineHeight,
    )
}

/** Appends [state], such as `open`, in small caps, as the web UI's `.state`. */
fun AnnotatedString.Builder.appendState(state: String) {
    withStyle(SpanStyle(fontFeatureSettings = "smcp")) { append(state) }
}
