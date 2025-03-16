<?php

namespace SecretSanta\Repositories;

use SecretSanta\Database\DataMapper;
use SecretSanta\Models\GiftAssignment;
use SecretSanta\Models\User;

/**
 * Repository class for managing gift assignments
 * 
 * Handles the database operations for gift assignments including
 * finding assignments and managing relationships between givers and receivers.
 */
class GiftAssignmentRepository extends DataMapper
{
    /**
     * Database table name for gift assignments
     * @var string
     */
    protected string $table = 'gift_assignments';
    
    /**
     * Entity class associated with this repository
     * @var string
     */
    protected string $entityClass = GiftAssignment::class;
    
    /**
     * Database columns for the gift assignments table
     * @var array
     */
    protected array $columns = [
        'id',
        'group_id',
        'giver_id',
        'receiver_id',
        'created_at',
        'notification_sent'
    ];

    /**
     * Find all gift assignments for a specific group
     * 
     * @param int $groupId The ID of the group
     * @return array Array of GiftAssignment objects
     */
    public function findByGroupId(int $groupId): array
    {
        return $this->findBy(['group_id' => $groupId]);
    }

    /**
     * Find all gift assignments where a user is the giver
     * 
     * @param int $giverId The ID of the giver
     * @return array Array of GiftAssignment objects
     */
    public function findByGiverId(int $giverId): array
    {
        return $this->findBy(['giver_id' => $giverId]);
    }

    /**
     * Find all gift assignments where a user is the receiver
     * 
     * @param int $receiverId The ID of the receiver
     * @return array Array of GiftAssignment objects
     */
    public function findByReceiverId(int $receiverId): array
    {
        return $this->findBy(['receiver_id' => $receiverId]);
    }

    /**
     * Find the gift assignment for a specific giver in a group
     * 
     * @param int $giverId The ID of the giver
     * @param int $groupId The ID of the group
     * @return GiftAssignment|null Returns the assignment or null if not found
     */
    public function findByGiverAndGroup(int $giverId, int $groupId): ?GiftAssignment
    {
        $result = $this->findBy([
            'giver_id' => $giverId,
            'group_id' => $groupId
        ]);

        return !empty($result) ? $result[0] : null;
    }

    /**
     * Find the gift assignment for a specific receiver in a group
     * 
     * @param int $receiverId The ID of the receiver
     * @param int $groupId The ID of the group
     * @return GiftAssignment|null Returns the assignment or null if not found
     */
    public function findByReceiverAndGroup(int $receiverId, int $groupId): ?GiftAssignment
    {
        $result = $this->findBy([
            'receiver_id' => $receiverId,
            'group_id' => $groupId
        ]);

        return !empty($result) ? $result[0] : null;
    }

    /**
     * Load the giver user object for a gift assignment
     * 
     * @param GiftAssignment $assignment The gift assignment to load the giver for
     * @return GiftAssignment Returns the updated assignment with giver relationship loaded
     */
    public function loadGiver(GiftAssignment $assignment): GiftAssignment
    {
        if ($assignment->getGiverId() === null) {
            return $assignment;
        }

        $userRepository = new UserRepository();
        $giver = $userRepository->find($assignment->getGiverId());

        return $assignment->setGiver($giver);
    }

    /**
     * Load the receiver user object for a gift assignment
     * 
     * @param GiftAssignment $assignment The gift assignment to load the receiver for
     * @return GiftAssignment Returns the updated assignment with receiver relationship loaded
     */
    public function loadReceiver(GiftAssignment $assignment): GiftAssignment
    {
        if ($assignment->getReceiverId() === null) {
            return $assignment;
        }

        $userRepository = new UserRepository();
        $receiver = $userRepository->find($assignment->getReceiverId());

        return $assignment->setReceiver($receiver);
    }

    /**
     * Load all relationships (giver and receiver) for a gift assignment
     * 
     * @param GiftAssignment $assignment The gift assignment to load relationships for
     * @return GiftAssignment Returns the updated assignment with all relationships loaded
     */
    public function loadRelationships(GiftAssignment $assignment): GiftAssignment
    {
        $this->loadGiver($assignment);
        $this->loadReceiver($assignment);

        return $assignment;
    }

    /**
     * Mark a gift assignment notification as sent
     * 
     * @param GiftAssignment $assignment The gift assignment to update
     * @return GiftAssignment Returns the updated assignment
     */
    public function markNotificationSent(GiftAssignment $assignment): GiftAssignment
    {
        $assignment->setNotificationSent(true);
        return $this->save($assignment);
    }
}
