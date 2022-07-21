from concurrent import futures
import datetime as dt
import os
import os.path
import time

import cv2

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
VID_NAME = ["vid1.mp4", "vid2.mp4"]
VID_PATH = list(map(lambda n: os.path.join(TMPFS_MOUNTPOINT, n), VID_NAME))


tmp = "/tmp/"


def video_processing(video_path):
    result_file_path = video_path + ".avi"

    video = cv2.VideoCapture(video_path)

    width = int(video.get(3))
    height = int(video.get(4))

    fourcc = cv2.VideoWriter_fourcc(*"XVID")
    out = cv2.VideoWriter(result_file_path, fourcc, 20.0, (width, height))

    while video.isOpened():
        ret, frame = video.read()

        if ret:
            gray_frame = cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY)
            tmp_file_path = tmp + "tmp.jpg"
            cv2.imwrite(tmp_file_path, gray_frame)
            gray_frame = cv2.imread(tmp_file_path)
            out.write(gray_frame)
        else:
            break

    video.release()
    out.release()
    return result_file_path


class VideoProcessing(fbpml_grpc.OneArgumentServicer):
    def Bench(
        self, request: fbpml.OneArgumentRequest, context: grpc.ServicerContext
    ):
        response_start = time.time()
        response_duration = Duration()

        # Apparently, vid2 needs twice the time vid1 does (~550ms vs ~1400ms)
        idx = request.arg if request.arg and 0 <= request.arg <= 1 else 1

        # Download the input video from MinIO
        minio_client = Minio(
            MINIO_ADDRESS,
            access_key=ACCESS_KEY,
            secret_key=SECRET_KEY,
            secure=False,
        )
        minio_client.fget_object(BUCKET_NAME, VID_NAME[idx], VID_PATH[idx])

        out_file_path = video_processing(VID_PATH[idx])

        # Upload the output video to MinIO
        minio_client.fput_object(
            BUCKET_NAME, os.path.basename(out_file_path), out_file_path
        )

        response_duration.FromTimedelta(
            dt.timedelta(seconds=time.time() - response_start)
        )
        return fbpml.ServiceResponse(response_duration=response_duration)


def serve():
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    fbpml_grpc.add_OneArgumentServicer_to_server(VideoProcessing(), server)
    server.add_insecure_port("[::]:50051")
    server.start()
    server.wait_for_termination()


if __name__ == "__main__":
    serve()
