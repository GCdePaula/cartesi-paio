ACCOUNT_ADDRESS=$(curl -H "Content-Type: application/json" \
                       -X POST --data \
                       '{"jsonrpc":"2.0", "method":"eth_accounts", "params":[],"id":1}' \
                       127.0.0.1:8545 \
                      | jaq -r '.result[0]')

SEQUENCER_ADDRESS=$(tq --file config.toml '.sequencer_address')

echo $ACCOUNT_ADDRESS

curl -H "Content-Type: application/json" -X POST --data \
     '{"jsonrpc":"2.0",
       "method":"eth_accounts",
       "params":[],
       "id":3}' 127.0.0.1:8545

echo -e "\n\n"

echo "aa: " $ACCOUNT_ADDRESS
echo "sa: " $SEQUENCER_ADDRESS

echo -e "\n\n"

echo "Balance before: "
cast balance $SEQUENCER_ADDRESS

cast send $SEQUENCER_ADDRESS \
     --from $ACCOUNT_ADDRESS \
     --unlocked \
     --value 1ether \
     --rpc-url 127.0.0.1:8545

echo "Balance after: "
cast balance $SEQUENCER_ADDRESS
