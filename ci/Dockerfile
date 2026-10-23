FROM rust:1.93-slim-trixie

RUN apt-get update && apt-get install -y --no-install-recommends \
        curl \
        ca-certificates \
        gnupg \
    && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs tini \
    && rm -rf /var/lib/apt/lists/*

COPY tuta-bridge /stonebridge/tuta-bridge

COPY stone-bridge/stonebridge /stonebridge/server

EXPOSE 2525

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
RUN chown -R 1000:1000 /stonebridge

RUN useradd -m mta
USER mta
ENV HOME=/home/mta
WORKDIR /home/mta
RUN mkdir -p /home/mta/.config/stonebridge
ENTRYPOINT ["/usr/bin/tini", "--", "/entrypoint.sh"]
