
fail() {
	printf "%s\n" "$1" >&2
	exit 1
}

not_empty() {
	test -z "$1" && fail "expected $2"
}


not_empty "$GITHUB_REF" GITHUB_REF
not_empty "$OS_NAME" OS_NAME


if [ "$OS_NAME" = "ubuntu" ]; then
  OS_NAME=linux
  target=x86_64-unknown-linux-musl/
  exe=""
elif [ "$OS_NAME" == "linux-aarch64" ]; then
  OS_NAME=linux_aarch64
  target=aarch64-unknown-linux-musl/
  exe=""
elif [ "$OS_NAME" == "macos" ]; then
  OS_NAME=osx
elif [ "$OS_NAME" == "windows" ]; then
  exe=".exe"
else
  fail "unknown os name: $OS_NAME"
fi

TAG_NAME="${GITHUB_REF##*/}"

generate_asset() {
  local asset_type=$1
  local bins="$2"
  local non_exe="$3"
  local TARGET_DIR=releases/golem-${asset_type}-${OS_NAME}-${TAG_NAME}
  mkdir -p "$TARGET_DIR"
  for component in $bins $non_exe; do
    strip -x target/${target}release/${component}${exe}
  done
  for bin in $bins; do
    cp "target/${target}release/${bin}${exe}" "$TARGET_DIR/"
  done
  if test -n "$non_exe"; then
    for bin in $non_exe; do
      cp "target/${target}release/${bin}" "$TARGET_DIR/"
    done
  fi
  if [ "$OS_NAME" = "windows" ]; then
    echo "::set-output name=${asset_type}Artifact::golem-${asset_type}-${OS_NAME}-${TAG_NAME}.zip"
    echo "::set-output name=${asset_type}Media::application/zip"
    (cd "$TARGET_DIR" && 7z a "../golem-${asset_type}-${OS_NAME}-${TAG_NAME}.zip" * )
  else
    echo "::set-output name=${asset_type}Artifact::golem-${asset_type}-${OS_NAME}-${TAG_NAME}.tar.gz"
    echo "::set-output name=${asset_type}Media::application/tar+gzip"
    (cd releases && tar czf "golem-${asset_type}-${OS_NAME}-${TAG_NAME}.tar.gz" "golem-${asset_type}-${OS_NAME}-${TAG_NAME}")
    du -h "releases/golem-${asset_type}-${OS_NAME}-${TAG_NAME}.tar.gz"
  fi
}

generate_asset "runtime" "ya-runtime-ai"
generate_asset "framework" "dummy" "ya-dummy-ai.json"
