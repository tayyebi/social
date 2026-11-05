package tld.android.social.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val LightColors = lightColorScheme(
    primary = Color(0xFF2E5AAC),
    onPrimary = Color(0xFFFFFFFF),
    primaryContainer = Color(0xFFD8E2FF),
    onPrimaryContainer = Color(0xFF0B1B3D),
    secondary = Color(0xFF3F6B4F),
    secondaryContainer = Color(0xFFD5E8D9),
    background = Color(0xFFF7F7FA),
    surface = Color(0xFFFFFFFF),
    surfaceVariant = Color(0xFFE6E7ED),
)

private val DarkColors = darkColorScheme(
    primary = Color(0xFF9DB9F0),
    onPrimary = Color(0xFF10203C),
    primaryContainer = Color(0xFF2A3F63),
    onPrimaryContainer = Color(0xFFD8E2FF),
    secondary = Color(0xFF9AC7A6),
    secondaryContainer = Color(0xFF2A3F31),
    background = Color(0xFF101216),
    surface = Color(0xFF171A20),
    surfaceVariant = Color(0xFF2A2E36),
)

@Composable
fun SocialMailTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    MaterialTheme(
        colorScheme = if (darkTheme) DarkColors else LightColors,
        content = content,
    )
}
