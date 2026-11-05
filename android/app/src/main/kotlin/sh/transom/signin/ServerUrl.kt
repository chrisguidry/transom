package sh.transom.signin

import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

/**
 * The server's URL from what a person typed, such as `transom.example.com/`, as
 * `https://transom.example.com`: `https://` when they gave no scheme, and no trailing slash, query,
 * or fragment, since the app adds its own paths to it. Null when it is no `http` or `https` URL.
 */
internal fun serverUrl(typed: String): String? {
    val trimmed = typed.trim()
    if (trimmed.isEmpty()) return null
    val withScheme = if ("://" in trimmed) trimmed else "https://$trimmed"
    val url = withScheme.toHttpUrlOrNull() ?: return null
    return url.newBuilder().query(null).fragment(null).build().toString().trimEnd('/')
}
