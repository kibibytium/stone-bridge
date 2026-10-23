.PHONY: build-server build-tuta build clean

build-server:
	mkdir ci/stone-bridge
	cd stone-bridge && cargo build --release
	cp stone-bridge/target/release/stonebridge ci/stone-bridge

build-tuta:
	cd tuta-bridge && node make.js
	cp -r tuta-bridge/build ci/tuta-bridge

build: build-server build-tuta

docker:
	cd ci && docker build -t stonebridge:latest .

clean:
	rm -rf ci/tuta-bridge tuta-bridge/build && rm -rf ci/stone-bridge
	cd stone-bridge && cargo clean
