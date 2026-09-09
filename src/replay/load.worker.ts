self.onmessage = async (event: MessageEvent<string>) => {
  try {
    const response = await fetch(event.data);
    if (!response.ok) throw new Error(`replay file: HTTP ${response.status}`);
    self.postMessage({ data: await response.json() });
  } catch (error) {
    self.postMessage({ error: error instanceof Error ? error.message : String(error) });
  }
};
