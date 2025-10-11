# Imscale Service

This is a simple image scaling service written in Rust. It uses Axum for the web server and the `image` crate for image processing.

## Features

- Dynamic image resizing with width and height parameters.
- Option to preserve the aspect ratio.
- Serves an example HTML page to demonstrate the service.

## Environment Variables

- `IMAGE_DIR`: The directory from which images are served. Defaults to `images`. The `images` directory is provided with this repository and contains a `placeholder.png` image that can be used for testing.

  Example:

  ```bash
  IMAGE_DIR=/path/to/your/images cargo run
  ```



1. **Run the service:**

   ```bash
   cargo run
   ```

2. **Access the service:**

   Open your browser and navigate to `http://localhost:3000`. You will see an example HTML page that demonstrates the image scaling service.

   The image is requested with the following URL:

   `http://localhost:3000/images/placeholder.png?width=<width>&height=<height>&preserve_aspect_ratio=true`

   - `width`: The desired width of the image.
   - `height`: The desired height of the image.
   - `preserve_aspect_ratio`: (Optional) Set to `true` to preserve the aspect ratio.
