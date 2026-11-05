package sh.transom.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.unit.dp
import sh.transom.R
import sh.transom.theme.BodyLineHeight
import sh.transom.theme.PageHeading

/** transom's own license, then each library the app ships with its license, linked to its text. */
@Composable
fun LicensesScreen(modifier: Modifier = Modifier) {
    Column(
        modifier
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp, vertical = 24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        PageHeading(stringResource(R.string.licenses), Modifier.padding(top = 4.dp))
        Text(linked(stringResource(R.string.licenses_transom), TRANSOM_LICENSE))
        Text(
            stringResource(R.string.licenses_libraries),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            lineHeight = BodyLineHeight,
        )
        LIBRARIES.forEach { library ->
            Column {
                Text(library.name, fontWeight = FontWeight.SemiBold, lineHeight = BodyLineHeight)
                Text(linked(library.license, library.url), lineHeight = BodyLineHeight)
            }
        }
    }
}

/** [text] as a link to [url], underlined in the accent, as the web's links are. */
@Composable
private fun linked(text: String, url: String): AnnotatedString {
    val style =
        TextLinkStyles(
            SpanStyle(MaterialTheme.colorScheme.primary, textDecoration = TextDecoration.Underline)
        )
    return buildAnnotatedString { withLink(LinkAnnotation.Url(url, style)) { append(text) } }
}

private const val TRANSOM_LICENSE = "https://github.com/chrisguidry/transom/blob/main/LICENSE"
