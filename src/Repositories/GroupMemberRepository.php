<?php

namespace SecretSanta\Repositories;

use SecretSanta\Database\DataMapper;
use SecretSanta\Models\GroupMember;

class GroupMemberRepository extends DataMapper
{
    protected string $table = 'group_members';
    protected string $entityClass = GroupMember::class;
    protected array $columns = [
        'id',
        'group_id',
        'user_id',
        'joined_at'
    ];

    public function findByGroupId(int $groupId): array
    {
        return $this->findBy(['group_id' => $groupId]);
    }

    public function findByUserId(int $userId): array
    {
        return $this->findBy(['user_id' => $userId]);
    }

    public function findByGroupAndUser(int $groupId, int $userId): ?GroupMember
    {
        $result = $this->findBy([
            'group_id' => $groupId,
            'user_id' => $userId
        ]);

        return !empty($result) ? $result[0] : null;
    }

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

    public function removeMember(int $groupId, int $userId): bool
    {
        $member = $this->findByGroupAndUser($groupId, $userId);

        if (!$member) {
            return false;
        }

        return $this->delete($member);
    }
}
