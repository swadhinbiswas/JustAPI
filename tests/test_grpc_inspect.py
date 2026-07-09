from justapi.app import JustAPIApp
from tests.proto.gen import helloworld_pb2, helloworld_pb2_grpc

class GreeterServicer(helloworld_pb2_grpc.GreeterServicer):
    def SayHello(self, request, context):
        return helloworld_pb2.HelloReply(message="Hello from Python!")

def test_grpc_inspect():
    app = JustAPIApp()
    app.set_grpc_addr("127.0.0.1:50051")
    app.add_grpc_service(GreeterServicer(), helloworld_pb2_grpc.add_GreeterServicer_to_server)
    print("gRPC service registered successfully!")
