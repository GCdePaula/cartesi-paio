echo "Getting nonce for user"
curl -X GET \
     -i \
     -H "Content-type: application/json" \
     -H "Accept: application/json" \
     -d '{"user": "0x0000000000000000000000000000000000000099", "application": "0x0000000000000000000000000000000000000003"}' \
     "0.0.0.0:3000/nonce"
echo -e "\n"

echo "Getting gas price"
curl -X GET \
     -i \
     -H "Content-type: application/json" \
     -H "Accept: application/json" \
     "0.0.0.0:3000/gas"
echo -e "\n"

echo "Get domain"
curl -X GET \
     -i \
     -H "Content-type: application/json" \
     -H "Accept: application/json" \
     "0.0.0.0:3000/domain"
echo -e "\n"

echo "Get batch"
curl -X GET \
     -i \
     -H "Content-type: application/json" \
     -H "Accept: application/json" \
     "0.0.0.0:3000/batch"
echo -e "\n"
