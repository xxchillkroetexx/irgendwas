<?php

namespace SecretSanta\Repositories;

use SecretSanta\Database\DataMapper;
use SecretSanta\Models\GroupMember;

/**
 * Repository class for managing group members
 * 
 * Handles the database operations for group members including
 * finding, adding and removing members from groups.
 */
class GroupMemberRepository extends DataMapper
{
    /**
     * Database table name for group members
     * @var string
     */
    protected string $table = 'group_members';
    
    /**
     * Entity class associated with this repository
     * @var string
     */
    protected string $entityClass = GroupMember::class;
    
    /**
     * Database columns for the group members table
     * @var array
     */
    protected array $columns = [
        'id',
        'group_id',
        'user_id',
        'joined_at'
    ];

    /**
     * Find all members of a specific group
     * 
     * @param int $groupId The ID of the group to find members for
     * @return array Array of GroupMember objects
     */
    public function findByGroupId(int $groupId): array
    {
        return $this->findBy(['group_id' => $groupId]);
    }

    /**
     * Find all groups a user is a member of
     * 
     * @param int $userId The ID of the user to find groups for
     * @return array Array of GroupMember objects
     */
    public function findByUserId(int $userId): array
    {
        return $this->findBy(['user_id' => $userId]);
    }

    /**
     * Find a specific group membership for a user and group
     * 
     * @param int $groupId The ID of the group
     * @param int $userId The ID of the user
     * @return GroupMember|null Returns the membership object or null if not found
     */
    public function findByGroupAndUser(int $groupId, int $userId): ?GroupMember
    {
        $result = $this->findBy([
            'group_id' => $groupId,
            'user_id' => $userId
        ]);

        return !empty($result) ? $result[0] : null;
    }

    /**
     * Add a user to a group
     * 
     * @param int $groupId The ID of the group to add the user to
     * @param int $userId The ID of the user to add to the group
     * @return GroupMember|null Returns the new member object or null if already a member
     */
    public function addMember(int $groupId, int $userId): ?GroupMember
    {
        // Check if already a member
        if ($this->findByGroupAndUser($groupId, $userId)) {
            return null;
        }

        $member = new GroupMember();
        $member->setGroupId($groupId)
            ->setUserId($userId)
            ->setJoinedAt(date('Y-m-d H:i:s'));

        return $this->save($member);
    }

    /**
     * Remove a user from a group
     * 
     * @param int $groupId The ID of the group to remove the user from
     * @param int $userId The ID of the user to remove from the group
     * @return bool True if successfully removed, false if not a member
     */
    public function removeMember(int $groupId, int $userId): bool
    {
        $member = $this->findByGroupAndUser($groupId, $userId);

        if (!$member) {
            return false;
        }

        return $this->delete($member);
    }
}
