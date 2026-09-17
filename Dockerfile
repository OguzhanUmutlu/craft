# ==============================================================================
# Craft Production Container Image
# Runtime: Eclipse Temurin Java 21 LTS + Craft Supervisor Daemon
# ==============================================================================
FROM eclipse-temurin:21-jre-noble

LABEL org.opencontainers.image.title="Craft" \
      org.opencontainers.image.description="High-performance Minecraft Server Management & Background Daemon" \
      org.opencontainers.image.source="https://github.com/OguzhanUmutlu/craft"

# Install essential runtime tools, init supervisor (tini), gosu, and Bedrock dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    tar \
    gzip \
    procps \
    tini \
    gosu \
    libcurl4 \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create craft system user & group (replacing default ubuntu user if present)
RUN (id -u ubuntu >/dev/null 2>&1 && userdel -r ubuntu 2>/dev/null || true) && \
    (getent group ubuntu >/dev/null 2>&1 && groupdel ubuntu 2>/dev/null || true) && \
    groupadd -g 1000 craft && \
    useradd -u 1000 -g craft -m -s /bin/bash craft

# Set up Craft data volume and environment
ENV CRAFT_HOME=/craft
ENV PATH="/usr/local/bin:${PATH}"

RUN mkdir -p /craft && chown -R craft:craft /craft

# Copy compiled craft binary
COPY target/release/craft /usr/local/bin/craft
RUN chmod +x /usr/local/bin/craft

# Copy entrypoint script
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# Minecraft standard ports:
# 25565     - Java Edition default
# 19132/udp - Bedrock Edition default
# 25575     - RCON console
# 8123      - Craft Daemon TCP / Dynmap
EXPOSE 25565 19132/udp 25575 8123

VOLUME ["/craft"]
WORKDIR /craft

ENTRYPOINT ["tini", "--", "/usr/local/bin/docker-entrypoint.sh"]
CMD ["daemon"]
