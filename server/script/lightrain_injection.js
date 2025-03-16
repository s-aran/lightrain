const connection = new WebSocket(
  "ws://127.0.0.1:5776/**lightrain_controller**/"
);

connection.onopen = (event) => {
  console.info(`open: ${event}`);

  connection.send("Hello Client");
};

connection.onerror = (error) => {
  console.error(`error: ${error}`);
};

connection.onmessage = (event) => {
  console.info(`message: ${event.data}`);
};

connection.onclose = (event) => {
  console.info(`close: ${event}`);
};
