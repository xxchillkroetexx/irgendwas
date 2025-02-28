<?php

namespace SecretSanta\Repositories;

use SecretSanta\Database\DataMapper;
use SecretSanta\Models\GiftAssignment;
use SecretSanta\Models\User;

class GiftAssignmentRepository extends DataMapper {
    protected string $table = 'gift_assignments';
    protected string $entityClass = GiftAssignment::class;
    protected array $columns = [
        'id', 'group_id', 'giver_id', 'receiver_id', 'created_at', 'notification_sent'
    ];
    
    public function findByGroupId(int $groupId): array {
        return $this->findBy(['group_id' => $groupId]);
    }
    
    public function findByGiverId(int $giverId): array {
        return $this->findBy(['giver_id' => $giverId]);
    }
    
    public function findByReceiverId(int $receiverId): array {
        return $this->findBy(['receiver_id' => $receiverId]);
    }
    
    public function findByGiverAndGroup(int $giverId, int $groupId): ?GiftAssignment {
        $result = $this->findBy([
            'giver_id' => $giverId,
            'group_id' => $groupId
        ]);
        
        return !empty($result) ? $result[0] : null;
    }
    
    public function findByReceiverAndGroup(int $receiverId, int $groupId): ?GiftAssignment {
        $result = $this->findBy([
            'receiver_id' => $receiverId,
            'group_id' => $groupId
        ]);
        
        return !empty($result) ? $result[0] : null;
    }
    
    public function loadGiver(GiftAssignment $assignment): GiftAssignment {
        if ($assignment->getGiverId() === null) {
            return $assignment;
        }
        
        $userRepository = new UserRepository();
        $giver = $userRepository->find($assignment->getGiverId());
        
        return $assignment->setGiver($giver);
    }
    
    public function loadReceiver(GiftAssignment $assignment): GiftAssignment {
        if ($assignment->getReceiverId() === null) {
            return $assignment;
        }
        
        $userRepository = new UserRepository();
        $receiver = $userRepository->find($assignment->getReceiverId());
        
        return $assignment->setReceiver($receiver);
    }
    
    public function loadRelationships(GiftAssignment $assignment): GiftAssignment {
        $this->loadGiver($assignment);
        $this->loadReceiver($assignment);
        
        return $assignment;
    }
    
    public function markNotificationSent(GiftAssignment $assignment): GiftAssignment {
        $assignment->setNotificationSent(true);
        return $this->save($assignment);
    }
}