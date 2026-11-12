"""The SDK's release identity, from the repository's release.toml.

`capsulet-xtask release set` rewrites these, and `release check` fails when they
disagree with release.toml. The package version is read from this file when the
package is built, so it is written down nowhere else.
"""

__version__ = "0.1.0"

# The Capsulet API versions this SDK is built against.
SUPPORTED_API_VERSIONS = ">=0.1.0, <0.2.0"
