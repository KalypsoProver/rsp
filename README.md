# Reth Succinct Processor Setup

Steps for setting up Reth Succinct Processor (RSP) on an Ubuntu instance.

## Steps

### 1. Clone the repository

```bash
git clone https://github.com/KalypsoProver/rsp.git
```

### 2. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 3. Setup SP1

Follow the official guide at [docs.succinct.xyz](https://docs.succinct.xyz/docs/sp1/getting-started/install).

Verify that the installation was successful with the following command:

```bash
cargo prove --version
```

### 4. Install m4, OpenSSL

> **Note:** `m4` and `openssl` are required for building the project.

```bash
sudo apt update
sudo apt install -y m4 libssl-dev
```

### 5. NVIDIA GPU drivers and toolkit setup

Make sure the NVIDIA GPU drivers are set up and the NVIDIA container toolkit is installed. Verify using the following commands:

```bash
nvidia-smi
```

```bash
nvcc --version
```

### 6. Build the eth-proofs and monitoring executables

For eth-proofs:

```bash
cargo build --release --bin eth-proofs
```

For monitoring:

```bash
cargo build --release --bin monitor
```

### 7. Setup systemd services

#### eth-proofs service:

Create a new systemd unit file for the eth-proofs service:

```bash
sudo vim /etc/systemd/system/eth-proofs.service
```

Paste the following configuration into the file:

```ini
[Unit]
Description=Eth Proofs Service
After=network.target

[Service]
ExecStart=<REPLACE WITH COMMAND TO EXECUTE ETH-PROOFS SERVICE>
Restart=always
RestartSec=5
EnvironmentFile=/root/.env
User=root
WorkingDirectory=/root

[Install]
WantedBy=multi-user.target
```

Reload systemd to pick up the new unit file, then enable and start the service:

```bash
# Reload systemd manager configuration
sudo systemctl daemon-reload

# Enable the service to start automatically on boot
sudo systemctl enable eth-proofs.service

# Start the service immediately
sudo systemctl start eth-proofs.service
```

Check the service status and follow the logs:

```bash
# Check the current status of the service
sudo systemctl status eth-proofs.service

# Stream live logs from the service
sudo journalctl -u eth-proofs.service -f
```

#### Monitor service

Add the following monitoring credentials to your `.env` file:

```
SUPABASE_API_KEY=<REPLACE_WITH_SUPABASE_API_KEY>
SUPABASE_URL=<REPLACE_WITH_SUPABASE_URL>
```

Create a new systemd unit file for the monitor service:

```bash
sudo vim /etc/systemd/system/eth-proofs-monitor.service
```

Paste the following configuration into the file:

```ini
[Unit]
Description=Eth Proofs Monitor
After=network.target

[Service]
ExecStart=/root/monitor
Restart=always
RestartSec=5
EnvironmentFile=/root/.env
User=root
WorkingDirectory=/root

[Install]
WantedBy=multi-user.target
```

Reload systemd to pick up the new unit file, then enable and start the service:

```bash
# Reload systemd manager configuration
sudo systemctl daemon-reload

# Enable the service to start automatically on boot
sudo systemctl enable eth-proofs-monitor.service

# Start the service immediately
sudo systemctl start eth-proofs-monitor.service
```

Check the service status and follow the logs:

```bash
# Check the current status of the service
sudo systemctl status eth-proofs-monitor.service

# Stream live logs from the service
sudo journalctl -u eth-proofs-monitor.service -f
```

### 8. Setup Cloudflare tunnel

Set up a [Cloudflare tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/) on port `9090` to expose the monitoring endpoint.