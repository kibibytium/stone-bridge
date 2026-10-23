# StoneBridge

StoneBridge is an SMTP bridge for Tuta Mail built for homelab use, enabling self-hosted services or automated scripts to
use Tuta for sending emails.

The SMTP server does not require credentials and accepts whatever SMTP credentials you send. The Tuta-bridge part of the
project is what logs into your Tuta account, reading the actual credentials from a config file.

The sender (the self-hosted app) does not see your Tuta credentials. As far as it is concerned, it is talking to an SMTP
server like any other

## How to use

To configure a service to use StoneBridge, go to its Email/SMTP settings page and enter the IP address of your
Stonebridge server and the port (2525 by default). Do not choose TLS or any Auth/Login options other than basic/plain.
Sometimes you can check an option to ignore TLS/certificate errors.

## Installation

There are pre-built docker images ready to use. Here is an example docker-compose file:

```yaml
---
services:
  stonebridge:
    image: "ghcr.io/kibibytium/stonebridge:latest"
    container_name: stonebridge
    hostname: stonebridge
    user: "1000:1000"
    volumes:
      - "./config:/home/mta/.config/stonebridge/config"
    restart: "unless-stopped"
    ports:
      - 2525:2525
```

Example config:

```
LOGIN_ADDRESS=your_tuta_address
SENDER_ADDRESS=sender_address
PASSWORD=your_password
# error, warn, info, debug
LOG_LEVEL = debug
```

## Building

- Checkout:
```bash
git clone https://github.com/kibibytium/stonbridge.git
cd stonebridge
git submodule update --init --recursive
```
- Check Tutanota's [BUILDING.md](https://github.com/tutao/tutanota/blob/master/doc/BUILDING.md) section for build dependencies.
- run `make build`

## Notes on Tutanota

From the Tutanota submodule, you need its TypeScript code, the crypto WebAssembly files
(from the `tuta-bridge/tutanota/build directory`) and the wasm-pack generated `crypto_primitives.js`.

### crypto_primitives

wasm-pack outputs `crypto_primitives.js` (`tutanota/src/platform-kit/crypto/crypto-primitives/crypto_primitives.js`)
built for a web target, but this project needs it to run in the Node.js runtime. The only issue is that it loads
`crypto_primitives_bg.wasm` using fetch, which doesn't work in Node.js.

The patch file `crypto_primitives.js.path` fixes this by replacing `fetch` wit `fs.readFile`.


## Current shortcommings

- The SMTP server and the MIME parser have been built from scratch and are not yet feature-complete.
  Please feel free to report any issues or feature requests.
- The Tuta part is still an ad-hoc proof-of-concept implementation:
    - it does not support multiple recipients
    - it does not support attachments.

## Tested Services

This setup has been tested and works perfectly with a wide range of self-hosted applications.
For example:

- Nextcloud
- Immich
- Vaultwarden
- Gitea/Forgejo
- 
