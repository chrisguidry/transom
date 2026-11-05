package sh.transom.testserver

import java.net.URI
import java.net.URLDecoder
import java.net.http.HttpResponse
import sh.transom.testserver.TestServer.Companion.APP_CALLBACK
import sh.transom.testserver.TestServer.Companion.ISSUER
import sh.transom.testserver.TestServer.Companion.encoded

/**
 * The trip a Custom Tab makes when the app signs [name] in, over plain HTTP: the sign-in page, its
 * link to the test issuer, the issuer's authorization endpoint, which signs [name] in at once, and
 * transom's callback, which answers with a redirect to [APP_CALLBACK] carrying the one-time code.
 * The redirects are followed by hand, carrying the cookie that binds the sign-in to its browser, so
 * the last one is read rather than followed.
 */
internal fun appSignIn(server: TestServer, name: String, challenge: String): String {
    val page =
        server.send(
            "${server.url}/login?app=${encoded(challenge)}&callback=${encoded(APP_CALLBACK)}",
            expected = 200,
        )
    val link =
        checkNotNull(issuerLink.find(page.body())) {
                "the sign-in page has no link to the issuer $ISSUER:\n${page.body()}"
            }
            .groupValues[1]
            .replace("&amp;", "&")
    val toIssuer = server.send("${server.url}$link", expected = 303)
    val cookie = toIssuer.headers().firstValue("set-cookie").orElseThrow().substringBefore(';')
    // The issuer signs in whoever the authorization request names.
    val authorize = "${toIssuer.location()}&subject=${encoded(name)}&name=${encoded(name)}"
    val back = server.send(authorize, expected = 303).location()
    val toApp = server.send(back, expected = 303, cookie = cookie).location()
    check(toApp.startsWith("$APP_CALLBACK?")) { "the sign-in ended at $toApp, not the app" }
    val code =
        checkNotNull(codeParameter.find(URI.create(toApp).rawQuery)) {
            "the app's callback has no code: $toApp"
        }
    return URLDecoder.decode(code.groupValues[1], Charsets.UTF_8)
}

private val issuerLink = Regex("""href="(/login/$ISSUER\?[^"]*)"""")
private val codeParameter = Regex("""(?:^|&)code=([^&]*)""")

private fun TestServer.send(
    url: String,
    expected: Int,
    cookie: String? = null,
): HttpResponse<String> {
    val request = get(url).header("Accept", "text/html")
    cookie?.let { request.header("Cookie", it) }
    val response = http.send(request.build(), HttpResponse.BodyHandlers.ofString())
    check(response.statusCode() == expected) {
        "GET $url answered ${response.statusCode()}, not $expected:\n${response.body()}"
    }
    return response
}

private fun HttpResponse<*>.location(): String = headers().firstValue("location").orElseThrow()
