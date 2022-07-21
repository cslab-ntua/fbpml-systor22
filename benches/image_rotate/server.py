from concurrent import futures
import datetime as dt
import os
import os.path
import time

from PIL import Image

from google.protobuf.duration_pb2 import Duration
import grpc

import functionbench_pmem_local_pb2 as fbpml
import functionbench_pmem_local_pb2_grpc as fbpml_grpc

from minio import Minio


MINIO_ADDRESS = os.getenv("MINIO_ADDRESS")
ACCESS_KEY = "minioroot"
SECRET_KEY = "minioroot"
BUCKET_NAME = "fbpml"

TMPFS_MOUNTPOINT = "/writable_tmpfs"
IMG_NAME = ["img1.jpeg", "img2.jpeg", "img3.jpeg"]
IMG_PATH = list(map(lambda n: os.path.join(TMPFS_MOUNTPOINT, n), IMG_NAME))


class ImageRotate(fbpml_grpc.OneArgumentServicer):
    def Bench(
        self, request: fbpml.OneArgumentRequest, context: grpc.ServicerContext
    ):
        response_start = time.time()
        response_duration = Duration()

        idx = request.arg if request.arg and 0 <= request.arg <= 2 else 2

        # Download the input image from MinIO
        minio_client = Minio(
            MINIO_ADDRESS,
            access_key=ACCESS_KEY,
            secret_key=SECRET_KEY,
            secure=False,
        )
        minio_client.fget_object(BUCKET_NAME, IMG_NAME[idx], IMG_PATH[idx])

        img = Image.open(IMG_PATH[idx])
        _ = img.transpose(Image.ROTATE_90)

        response_duration.FromTimedelta(
            dt.timedelta(seconds=time.time() - response_start)
        )
        return fbpml.ServiceResponse(response_duration=response_duration)


def serve():
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    fbpml_grpc.add_OneArgumentServicer_to_server(ImageRotate(), server)
    server.add_insecure_port("[::]:50051")
    server.start()
    server.wait_for_termination()


if __name__ == "__main__":
    serve()
