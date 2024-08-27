import "./style.css"
import { listProviders } from "./providers.ts"

document.querySelector<HTMLDivElement>("#app")!.innerHTML = `
  <h2>EIP-712 Signing with MetaMask</h>
  <div>
    <p>Assinar tx</p>
      <button>get domain</button>
      <button>get nonce</button>
      <button>dapp address</button>
      <button>data</button>
      <button>sign tx</button>
      <button>send tx</button>
    <div id="providerButtons"></div>
  </div>
`

listProviders(document.querySelector<HTMLDivElement>("#providerButtons")!)
