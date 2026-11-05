package sh.transom.settings

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The hand-kept [LIBRARIES] against every module the release build ships, which Gradle reads from
 * the release build's resolved dependencies and passes as `transom.shipped`.
 */
class LicensesTest {
    private val shippedGroups =
        checkNotNull(System.getProperty("transom.shipped")) {
                "the system property transom.shipped is not set: run the tests with Gradle"
            }
            .split(",")
            .map { it.substringBefore(":") }
            .toSet()

    private val patterns = LIBRARIES.flatMap { it.groups }

    private fun matches(pattern: String, group: String) =
        if (pattern.endsWith(".*")) group.startsWith(pattern.removeSuffix("*"))
        else group == pattern

    @Test
    fun everyShippedLibraryHasItsLicense() {
        assertEquals(
            emptyList<String>(),
            shippedGroups.filter { group -> patterns.none { matches(it, group) } },
        )
    }

    @Test
    fun everyListedLibraryIsShipped() {
        assertEquals(
            emptyList<String>(),
            patterns.filter { pattern -> shippedGroups.none { matches(pattern, it) } },
        )
    }
}
