#!/usr/bin/env bash
#
# Build the Android release APK (debug-signed) with the SAMMIE launcher icon.
#
# This does two things a plain `dx build` can't:
#
#  1. Launcher icon. dx 0.7 has no config to set the Android launcher icon and overwrites
#     its own default (Android robot) on every build (Dioxus bug #3685), so we overlay the
#     pre-made SAMMIE icons from android/icons/ ourselves.
#
#  2. Release variant. dx only ever runs gradle's *debug* buildType for Android, even with
#     --release (which only affects the Rust profile). To ship the smaller release variant
#     we run `assembleRelease` ourselves, with:
#       - minify OFF: ProGuard would strip our Kotlin JNI methods (only called from Rust),
#         silently breaking the installer.
#       - the debug signing key: an APK must be signed to install at all, and signing with
#         the auto-generated debug key means no keystore/secrets to manage. Fine for
#         sideloading; not a Play-Store release identity.
#
# When Dioxus fixes #3685, the icon overlay can go away (set it in Dioxus.toml instead).
#
set -euo pipefail
cd "$(dirname "$0")/.."

# Toolchain. Falls back to the Homebrew install locations if the env isn't already set.
export JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home}"
export ANDROID_HOME="${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/27.2.12479018}"
export PATH="$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$PATH"

echo "==> dx build --release (release Rust + generated gradle project)"
dx build --release --platform android --target aarch64-linux-android "$@"

GEN=target/dx/CobaltInstaller/release/android/app
RES=$GEN/app/src/main/res
ICONS=android/icons

echo "==> overlaying the SAMMIE launcher icon"
cp "$ICONS/ic_launcher-mdpi.webp"    "$RES/mipmap-mdpi/ic_launcher.webp"
cp "$ICONS/ic_launcher-hdpi.webp"    "$RES/mipmap-hdpi/ic_launcher.webp"
cp "$ICONS/ic_launcher-xhdpi.webp"   "$RES/mipmap-xhdpi/ic_launcher.webp"
cp "$ICONS/ic_launcher-xxhdpi.webp"  "$RES/mipmap-xxhdpi/ic_launcher.webp"
cp "$ICONS/ic_launcher-xxxhdpi.webp" "$RES/mipmap-xxxhdpi/ic_launcher.webp"
mkdir -p "$RES/drawable-nodpi"
cp "$ICONS/sammie.webp" "$RES/drawable-nodpi/sammie.webp"
cat > "$RES/mipmap-anydpi-v26/ic_launcher.xml" <<'XML'
<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@android:color/white" />
    <foreground android:drawable="@drawable/sammie" />
</adaptive-icon>
XML

# dx regenerates build.gradle.kts on every build, so these run against a fresh file each
# time. Turn minify off and sign the release variant with the debug key.
echo "==> release buildType: minify off + debug signing"
perl -i -pe 's/isMinifyEnabled = true/isMinifyEnabled = false/' "$GEN/app/build.gradle.kts"
perl -i -pe 's/(getByName\("release"\)\s*\{)/$1\n            signingConfig = signingConfigs.getByName("debug")/' "$GEN/app/build.gradle.kts"

echo "==> assembleRelease"
( cd "$GEN" && ./gradlew assembleRelease -q )

APK=$GEN/app/build/outputs/apk/release/app-release.apk
echo "==> done: $APK"
