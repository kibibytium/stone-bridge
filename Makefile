.PHONY: build-server build-tuta bridge tutanota patch-crypto clean

build-server:
	mkdir ci/stone-bridge
	cd stone-bridge && cargo build --release
	cp stone-bridge/target/release/stonebridge ci/stone-bridge

patch-crypto:
	cd tuta-bridge && patch -p0 < crypto_primitives.js.patch

tutanota:
	cd tuta-bridge/tutanota && npm ci --allow-git=all && node make

build-tuta:
	cd tuta-bridge && npm ci && node make.js
	cp -r tuta-bridge/build ci/tuta-bridge

bridge: build-server build-tuta

docker:
	cd ci && docker build -t ghcr.io/kibibytium/stone-bridge:latest .

build: tutanota patch-crypto bridge

clean:
	rm -rf ci/tuta-bridge tuta-bridge/build && rm -rf ci/stone-bridge
	cd stone-bridge && cargo clean
