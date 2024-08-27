declare global {
  interface WindowEventMap {
    "eip6963:announceProvider": CustomEvent
  }
}

// Connect to the selected provider using eth_requestAccounts.
const connectWithProvider = async (
  wallet: EIP6963AnnounceProviderEvent["detail"]
) => {
  try {
    await wallet.provider.request({ method: "eth_requestAccounts" })
  } catch (error) {
    console.error("Failed to connect to provider:", error)
  }
}

// Display detected providers as connect buttons.
export function listProviders(element: HTMLDivElement) {
  window.addEventListener(
    "eip6963:announceProvider",
    (event: EIP6963AnnounceProviderEvent) => {
      const button = document.createElement("button")

      button.innerHTML = `
        <img src="${event.detail.info.icon}" alt="${event.detail.info.name}" />
        <div>${event.detail.info.name}</div>
      `

      // Call connectWithProvider when a user selects the button.
      button.onclick = () => connectWithProvider(event.detail)
      element.appendChild(button)
    }
  )

  // Notify event listeners and other parts of the dapp that a provider is requested.
  window.dispatchEvent(new Event("eip6963:requestProvider"))
}