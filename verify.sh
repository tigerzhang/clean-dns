#!/bin/bash
set -e

# Start Server
./target/debug/clean-dns -c config.yaml &
SERVER_PID=$!
echo "Server started with PID $SERVER_PID"

sleep 5

# Query Domain
echo "Querying apple.com..."
dig @127.0.0.1 -p 5335 apple.com +short

sleep 1

# Query Again (Cache Hit)
echo "Querying apple.com again..."
dig @127.0.0.1 -p 5335 apple.com +short

sleep 1

# Query Stats
echo "Fetching stats..."
curl -s http://127.0.0.1:3002/stats | python3 -m json.tool

# Cleanup
kill $SERVER_PID
echo "Server killed"
