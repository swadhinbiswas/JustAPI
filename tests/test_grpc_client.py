import grpc
from tests.proto.gen import helloworld_pb2, helloworld_pb2_grpc

def run():
    with grpc.insecure_channel('127.0.0.1:50051') as channel:
        stub = helloworld_pb2_grpc.GreeterStub(channel)
        print("Calling SayHello...")
        response = stub.SayHello(helloworld_pb2.HelloRequest(name='JustAPI'))
        print("Greeter client received: " + response.message)

if __name__ == '__main__':
    run()
