# StoneBridge

Stone bridge is an SMTP bridge for Tuta Mail built for homelab use, enabling self-hosted services, or
automated scripts to use Tuta for sending emails.

The SMTP server does not require credentials and it accepts whatever SMTP credentials you send. The Tuta-bridge
part of the project is what logs you in to your Tuta accounts. The credentials are read from a config file.

The sender (the self-hosted app) does not see your Tuta credentials. As far as it is conserned, it talks to
an SMTP server like any other.

## How to use

If you want to configure a service to use Stonebridge, go to its Email/SMTP settings page and enter the
ip address of your Stonebridge server and the port (2525 by default). Do not choose TLS or any Auth/Login options
other than basic/plain. Sometimes you can check an option to ignore TLS/certificate errors.

This a basic, but robust SMTP server. It is not fully compliant nor
ready for broader general use. I implement support for what is needed
as I go.

## Current shortcommings

The SMTP server and the MIME parser have been built from scratch and are not yet feature complete.
Please feel free to report any issue or feature request.

## Tested Services

This setup has been tested and works perfectly with these following services:

- nextcloud
- immich
- vaultwarden
