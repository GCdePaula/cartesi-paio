echo "Getting nonce for user 10 in application 20"
curl -X GET \
     -i \
     -H "Content-type: application/json" \
     -H "Accept: application/json" \
     -d '{"user": 10, "application": 20}' \
     "0.0.0.0:3000/nonce"
echo -e "\n"

echo "Getting nonce for user 99 in application 3"
curl -X GET \
     -i \
     -H "Content-type: application/json" \
     -H "Accept: application/json" \
     -d '{"user": 99, "application": 3}' \
     "0.0.0.0:3000/nonce"
echo -e "\n"

echo "Getting gas price"
curl -X GET \
     -i \
     -H "Content-type: application/json" \
     -H "Accept: application/json" \
     "0.0.0.0:3000/gas"
echo -e "\n"

echo "Submitting transaction with temperos 20"
curl -X POST \
     -i \
     -H "Content-type: application/json" \
     -H "Accept: application/json" \
     -d '{"temperos": 20}' \
     "0.0.0.0:3000/transaction"
echo -e "\n"

echo "Submitting transaction with temperos -2"
curl -X POST \
     -i \
     -H "Content-type: application/json" \
     -H "Accept: application/json" \
     -d '{"temperos": -2}' \
     "0.0.0.0:3000/transaction"
echo -e "\n"

# curl -H "Content-Type: application/json" -d '{"username": "Fred"}' 0.0.0.0:3000/users
echo ""
