#!/usr/bin/env bash
set -euo pipefail

: "${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
: "${GITHUB_REF_NAME:?GITHUB_REF_NAME is required}"
: "${GITHUB_SHA:?GITHUB_SHA is required}"

encoded_tag="$(jq -rn --arg tag "$GITHUB_REF_NAME" '$tag | @uri')"
ref_json="$(gh api "repos/$GITHUB_REPOSITORY/git/ref/tags/$encoded_tag")"
object_type="$(jq -r '.object.type // empty' <<<"$ref_json")"
if [ "$object_type" != "tag" ]; then
  echo "::error::Release tag $GITHUB_REF_NAME is no longer an annotated tag."
  exit 1
fi

tag_object_sha="$(jq -r '.object.sha // empty' <<<"$ref_json")"
if ! [[ "$tag_object_sha" =~ ^[0-9a-f]{40}$ ]]; then
  echo "::error::GitHub did not return a valid annotated tag object for $GITHUB_REF_NAME."
  exit 1
fi

tag_json="$(gh api "repos/$GITHUB_REPOSITORY/git/tags/$tag_object_sha")"
verified="$(jq -r '.verification.verified // false' <<<"$tag_json")"
reason="$(jq -r '.verification.reason // "no verification reason returned"' <<<"$tag_json")"
if [ "$verified" != "true" ]; then
  echo "::error::Release tag $GITHUB_REF_NAME is not cryptographically verified by GitHub (reason: $reason)."
  exit 1
fi

target_type="$(jq -r '.object.type // empty' <<<"$tag_json")"
target_sha="$(jq -r '.object.sha // empty' <<<"$tag_json")"
if [ "$target_type" != "commit" ]; then
  echo "::error::Release tag $GITHUB_REF_NAME must point directly to a commit, not $target_type."
  exit 1
fi
if [ "$target_sha" != "$GITHUB_SHA" ]; then
  echo "::error::Verified release tag target $target_sha does not match the immutable workflow SHA $GITHUB_SHA."
  exit 1
fi

echo "Verified signed annotated release tag $GITHUB_REF_NAME at $GITHUB_SHA."
