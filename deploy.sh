if [ "$(uname -sm)" = "Linux x86_64" ]; then
    BUILD_TARGET_PATH="target"
else
    BUILD_TARGET_PATH="target/x86_64-unknown-linux-musl"
    BUILD_TARGET="x86_64-unknown-linux-musl"
fi

if [ "$1" = "encoder" ]; then
    SVC=encoder
    ORIGINAL_BINARY_NAME="encoder"
    ORIGINAL_BINARY_PATH="$BUILD_TARGET_PATH/release/$ORIGINAL_BINARY_NAME"
elif [ "$1" = "server" ]; then
    SVC=server
    ORIGINAL_BINARY_NAME="server"
    ORIGINAL_BINARY_PATH="$BUILD_TARGET_PATH/release/$ORIGINAL_BINARY_NAME"
else
    function chart_upgrade() {
    helm upgrade \
        --install \
        --namespace erish-dev \
        --create-namespace \
        -f ./_values.yaml \
        erish ./chart
    }

    chart_upgrade

    exit 0
fi

function build() {
    if [ "$BUILD_TARGET" ]; then
        cargo build --bin $ORIGINAL_BINARY_NAME --release --target $BUILD_TARGET
    else
        cargo build --bin $ORIGINAL_BINARY_NAME --release
    fi
}

function copy_binary_to_temp_dir() {
    BINARY_NAME="$(md5 -q $ORIGINAL_BINARY_PATH)"
    BINARY_PATH="$TEMP/$BINARY_NAME"
    cp $ORIGINAL_BINARY_PATH $TEMP
    mv "$TEMP/$ORIGINAL_BINARY_NAME" "$TEMP/$BINARY_NAME"
    chmod +x $BINARY_PATH
}

TEMP="$SVC/.temp_bin"

mkdir -p $TEMP

build

if [ $? -ne 0 ]; then
    exit $?
fi

VERSION="$(cat ./$SVC/version)"

copy_binary_to_temp_dir

if [ $? -ne 0 ]; then
    exit $?
fi

docker build \
    --platform linux/amd64 \
    --build-arg BINARY_FILE=".temp_bin/$BINARY_NAME" \
    --tag "syrlee/erish-$SVC:$VERSION" \
    --push ./$SVC


if [ $? -ne 0 ]; then
    rm -rf $BINARY_PATH
    exit $?
fi

# rm -rf $BINARY_PATH
rm -rf $TEMP
