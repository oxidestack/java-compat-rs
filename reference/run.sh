#!/bin/sh
# Regenerates tests/fixtures/{full,javabase}/*.json.
#   full:     a full JDK 26 (eclipse-temurin:26-jdk), jdk.localedata present
#   javabase: the MockServer 8.0.0 image's trimmed Temurin 26, java.base locale data only
#   jdk21:    Keycloak 26.5.3's OpenJDK 21 (quay.io/keycloak/keycloak:26.5.3): locale_numbers.json, locale_dates.json,
#             week_data.json, currency.json, charsets.json and regex.json only
#   (`run.sh regex` records regex.json alone; `run.sh localenumbers` / `localedates` / `currency` / `charsets` one set
#   in all three JDKs)
# Needs docker and a local javac; commons-lang3 3.20.0 from ~/.m2 for LocaleUtils.
set -e
here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
out="$here/out"
mkdir -p "$out/classes" "$out/lib" "$root/tests/fixtures/full" "$root/tests/fixtures/javabase" "$root/tests/fixtures/jdk21"
jar="$HOME/.m2/repository/org/apache/commons/commons-lang3/3.20.0/commons-lang3-3.20.0.jar"
cp "$jar" "$out/lib/"
javac --release 21 -nowarn -encoding UTF-8 -cp "$out/lib/commons-lang3-3.20.0.jar" -d "$out/classes" "$here"/src/*.java
flags="-Duser.language=en -Duser.country= -Duser.timezone=GMT --add-opens java.base/jdk.internal.math=ALL-UNNAMED"
if [ "$1" = regex ]; then
  docker run --rm -v "$out:/w" -v "$root/tests/fixtures/jdk21:/o" --entrypoint java quay.io/keycloak/keycloak:26.5.3 \
    $flags -cp /w/classes:/w/lib/commons-lang3-3.20.0.jar Fixtures /o regex
  exit 0
fi
docker run --rm -v "$out:/w" -v "$root/tests/fixtures/full:/o" eclipse-temurin:26-jdk \
  java $flags -cp /w/classes:/w/lib/commons-lang3-3.20.0.jar Fixtures /o "$@"
docker run --rm -v "$out:/w" -v "$root/tests/fixtures/javabase:/o" --entrypoint /usr/lib/jvm/temurin25-trimmed/bin/java \
  mockserver/mockserver:8.0.0 $flags -cp /w/classes:/w/lib/commons-lang3-3.20.0.jar Fixtures /o "$@"
for set in localenumbers localedates currency charsets; do
  if [ -z "$1" ] || [ "$1" = $set ]; then
    docker run --rm -v "$out:/w" -v "$root/tests/fixtures/jdk21:/o" --entrypoint java quay.io/keycloak/keycloak:26.5.3 \
      $flags -cp /w/classes:/w/lib/commons-lang3-3.20.0.jar Fixtures /o $set
  fi
done
