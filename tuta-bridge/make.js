import { rolldown } from "rolldown"
import { copyFileSync, mkdirSync } from "node:fs"

export async function build() {
	mkdirSync("build", { recursive: true })
	copyFileSync("crypto_wasm/argon2.wasm", "build/argon2.wasm")
	copyFileSync("crypto_wasm/liboqs.wasm", "build/liboqs.wasm")
	copyFileSync("crypto_wasm/crypto_primitives_bg.wasm", "build/crypto_primitives_bg.wasm")

	const bundle = await rolldown({
		input: { app: "src/main" },
		platform: "node",
	})
	await bundle.write({
		dir: `./build/`,
		format: "esm",
		codeSplitting: false,
	})
}

await build()
