# Deployment Guide

This document provides comprehensive deployment instructions for the Fossil Headers DB indexer service.

## Overview

The application consists of two main components:
- **Migration Service**: Runs database migrations using `Dockerfile.migrate`
- **Indexer Service**: Main application service using `Dockerfile.indexer`

## Prerequisites

### Required Environment Variables

```bash
# Database Configuration
DB_CONNECTION_STRING=postgresql://username:password@host:port/database
DATABASE_URL=postgresql://username:password@host:port/database  # For migrations

# Ethereum Node Configuration
NODE_CONNECTION_STRING=https://your-ethereum-rpc-endpoint

# Application Configuration
ROUTER_ENDPOINT=0.0.0.0:3000
RUST_LOG=info
INDEX_TRANSACTIONS=false
START_BLOCK_OFFSET=1024  # Optional: starting block offset
IS_DEV=false            # Set to false for production
```

### Infrastructure Requirements

- **Database**: PostgreSQL 16+ (managed service recommended)
- **Compute**: Minimum 1 vCPU, 2GB RAM for indexer service
- **Storage**: Block data grows over time, ensure adequate storage
- **Network**: Outbound HTTPS access to Ethereum RPC endpoint

## AWS ECS Deployment

### 1. Build and Push Docker Images

```bash
# Build images
docker build -f docker/Dockerfile.migrate -t your-repo/fossil-migrate:latest .
docker build -f docker/Dockerfile.indexer -t your-repo/fossil-indexer:latest .

# Push to ECR
aws ecr get-login-password --region us-west-2 | docker login --username AWS --password-stdin your-account.dkr.ecr.us-west-2.amazonaws.com
docker tag your-repo/fossil-migrate:latest your-account.dkr.ecr.us-west-2.amazonaws.com/fossil-migrate:latest
docker tag your-repo/fossil-indexer:latest your-account.dkr.ecr.us-west-2.amazonaws.com/fossil-indexer:latest
docker push your-account.dkr.ecr.us-west-2.amazonaws.com/fossil-migrate:latest
docker push your-account.dkr.ecr.us-west-2.amazonaws.com/fossil-indexer:latest
```

### 2. Database Setup

1. **Create RDS PostgreSQL Instance**:
   - Engine: PostgreSQL 16+
   - Instance class: db.t3.micro (minimum)
   - Storage: GP3 with at least 20GB
   - Enable automated backups
   - Configure security groups for ECS access

2. **Connection String Format**:
   ```
   postgresql://username:password@your-rds-endpoint:5432/database_name
   ```

### 3. ECS Task Definitions

#### Migration Task Definition

```json
{
  "family": "fossil-migrate",
  "requiresCompatibilities": ["FARGATE"],
  "networkMode": "awsvpc",
  "cpu": "256",
  "memory": "512",
  "executionRoleArn": "arn:aws:iam::account:role/ecsTaskExecutionRole",
  "containerDefinitions": [
    {
      "name": "migrate",
      "image": "your-account.dkr.ecr.region.amazonaws.com/fossil-migrate:latest",
      "essential": true,
      "environment": [
        {
          "name": "DATABASE_URL",
          "value": "postgresql://username:password@rds-endpoint:5432/database"
        }
      ],
      "logConfiguration": {
        "logDriver": "awslogs",
        "options": {
          "awslogs-group": "/ecs/fossil-migrate",
          "awslogs-region": "us-west-2",
          "awslogs-stream-prefix": "ecs"
        }
      }
    }
  ]
}
```

#### Indexer Task Definition

```json
{
  "family": "fossil-indexer",
  "requiresCompatibilities": ["FARGATE"],
  "networkMode": "awsvpc",
  "cpu": "1024",
  "memory": "2048",
  "executionRoleArn": "arn:aws:iam::account:role/ecsTaskExecutionRole",
  "containerDefinitions": [
    {
      "name": "indexer",
      "image": "your-account.dkr.ecr.region.amazonaws.com/fossil-indexer:latest",
      "essential": true,
      "portMappings": [
        {
          "containerPort": 3000,
          "protocol": "tcp"
        }
      ],
      "environment": [
        {
          "name": "DB_CONNECTION_STRING",
          "value": "postgresql://username:password@rds-endpoint:5432/database"
        },
        {
          "name": "NODE_CONNECTION_STRING",
          "value": "https://your-ethereum-rpc-endpoint"
        },
        {
          "name": "ROUTER_ENDPOINT",
          "value": "0.0.0.0:3000"
        },
        {
          "name": "RUST_LOG",
          "value": "info"
        },
        {
          "name": "INDEX_TRANSACTIONS",
          "value": "false"
        }
      ],
      "healthCheck": {
        "command": ["CMD-SHELL", "curl -f http://localhost:3000/health || exit 1"],
        "interval": 30,
        "timeout": 10,
        "retries": 3,
        "startPeriod": 60
      },
      "logConfiguration": {
        "logDriver": "awslogs",
        "options": {
          "awslogs-group": "/ecs/fossil-indexer",
          "awslogs-region": "us-west-2",
          "awslogs-stream-prefix": "ecs"
        }
      }
    }
  ]
}
```

### 4. ECS Service Configuration

#### Migration Service (One-time Task)

Run migrations as a one-time task before deploying the indexer:

```bash
aws ecs run-task \
    --cluster your-cluster \
    --task-definition fossil-migrate \
    --launch-type FARGATE \
    --network-configuration "awsvpcConfiguration={subnets=[subnet-xxx],securityGroups=[sg-xxx],assignPublicIp=ENABLED}"
```

#### Indexer Service (Long-running)

```json
{
  "serviceName": "fossil-indexer",
  "cluster": "your-cluster",
  "taskDefinition": "fossil-indexer",
  "desiredCount": 1,
  "launchType": "FARGATE",
  "networkConfiguration": {
    "awsvpcConfiguration": {
      "subnets": ["subnet-xxx", "subnet-yyy"],
      "securityGroups": ["sg-xxx"],
      "assignPublicIp": "ENABLED"
    }
  },
  "loadBalancers": [
    {
      "targetGroupArn": "arn:aws:elasticloadbalancing:region:account:targetgroup/fossil-indexer",
      "containerName": "indexer",
      "containerPort": 3000
    }
  ],
  "healthCheckGracePeriodSeconds": 300
}
```

## Local Development Deployment

For local development and testing:

```bash
# Start local environment
make dev-up

# Run migrations
docker compose -f docker/docker-compose.local.yml up migrations

# Start indexer
docker compose -f docker/docker-compose.local.yml up indexer

# Or use make commands
make run-indexer
```

## Migration Process

### Initial Deployment

1. **Run Database Migrations**:
   ```bash
   # Via ECS one-time task
   aws ecs run-task --cluster your-cluster --task-definition fossil-migrate
   
   # Or locally for testing
   make migrate DATABASE_URL="postgresql://username:password@host:port/database"
   ```

2. **Verify Migration Success**:
   ```bash
   # Connect to database and verify tables exist
   psql $DATABASE_URL -c "\dt"
   ```

3. **Deploy Indexer Service**:
   ```bash
   aws ecs create-service --cli-input-json file://indexer-service.json
   ```

### Updating Existing Deployment

1. **Update Images**:
   ```bash
   # Build and push new images with version tags
   docker build -f docker/Dockerfile.indexer -t your-repo/fossil-indexer:v1.1.0 .
   docker push your-account.dkr.ecr.region.amazonaws.com/fossil-indexer:v1.1.0
   ```

2. **Run New Migrations** (if any):
   ```bash
   aws ecs run-task --cluster your-cluster --task-definition fossil-migrate:latest
   ```

3. **Update Service**:
   ```bash
   # Update task definition with new image
   aws ecs update-service --cluster your-cluster --service fossil-indexer --task-definition fossil-indexer:latest
   ```

## Monitoring and Health Checks

### Health Endpoint

The indexer service exposes a health check endpoint at `/health` on port 3000.

### Key Metrics to Monitor

- **Database Connections**: Monitor connection pool usage
- **Block Processing Rate**: Blocks indexed per minute
- **RPC Call Success Rate**: Ethereum node connectivity
- **Memory Usage**: Monitor for memory leaks
- **Log Errors**: Watch for RPC failures and database errors

### CloudWatch Logs

Configure log groups:
- `/ecs/fossil-migrate` - Migration logs
- `/ecs/fossil-indexer` - Indexer application logs

## Security Considerations

1. **Database Access**: Use IAM database authentication when possible
2. **Secrets Management**: Store sensitive environment variables in AWS Secrets Manager
3. **Network Security**: Configure security groups to restrict database access
4. **Container Security**: Images are built with minimal attack surface

## Troubleshooting

### Common Issues

1. **Migration Failures**:
   - Check database connectivity
   - Verify DATABASE_URL format
   - Ensure database exists

2. **Indexer Startup Issues**:
   - Check all environment variables are set
   - Verify Ethereum RPC endpoint is accessible
   - Ensure migrations completed successfully

3. **Performance Issues**:
   - Monitor database connection limits
   - Check Ethereum RPC rate limits
   - Scale ECS service if needed

### Logs and Debugging

```bash
# View ECS service logs
aws logs get-log-events --log-group-name /ecs/fossil-indexer

# Check task status
aws ecs describe-tasks --cluster your-cluster --tasks task-id
```

## Cost Optimization

- Use Fargate Spot for non-critical workloads
- Right-size ECS tasks based on actual resource usage
- Consider reserved capacity for RDS instances
- Monitor CloudWatch costs and set up billing alarms